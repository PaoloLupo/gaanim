//! Recipes of the per-drawable vector fragments the 2D renderer composites.
//!
//! A [`FragmentRecipe`] holds every value a drawable's fragment is built
//! from, and [`build_fragment`] is the only code that turns one into Vello
//! drawing commands. The interactive preview, the headless export and the
//! playback bundles all build fragments through it, so a recorded recipe
//! draws exactly what the live scene drew.

use std::sync::Arc;

use gaanim_core::{kurbo, peniko};
use gaanim_scene::{RasterImage, StrokeBrush};

use crate::effects::{ChalkBrush, DropShadow, GaussianBlur, Glow, StrokeAlign, StrokeProfile};

/// Everything one drawable's fragment is built from.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FragmentRecipe {
    /// Visible outline (`Path2D`).
    pub path: Option<Arc<kurbo::BezPath>>,
    /// Untrimmed outline (`PathSource`) while a draw animation reveals `path`.
    pub source: Option<Arc<kurbo::BezPath>>,
    /// Fill paint.
    pub fill: Option<peniko::Brush>,
    /// Stroke paint and pen.
    pub stroke: Option<StrokeBrush>,
    /// Raster content clipped to `path`.
    pub raster: Option<RasterImage>,
    /// Whether a Lottie scene is drawn first. Its commands come from the
    /// Lottie player, not from the recipe.
    pub lottie: bool,
    pub shadow: Option<DropShadow>,
    pub glow: Option<Glow>,
    pub blur: Option<GaussianBlur>,
    /// `FillDrawProgress` of a write animation.
    pub fill_progress: Option<f32>,
    /// `WriteTipGlow::completion` of a write animation.
    pub completion: Option<f64>,
    pub stroke_align: StrokeAlign,
    pub stroke_profile: Option<StrokeProfile>,
    /// Pen correction inside zoomed coordinate views.
    pub stroke_view: Option<kurbo::Affine>,
    /// A camera view screen draws its stroke above what its camera sees, in
    /// a separate overlay.
    pub screen: bool,
    /// Chalk look of the fill and stroke.
    pub chalk: Option<ChalkBrush>,
}

/// Borrowed components a recipe is captured from.
pub(crate) struct FragmentParts<'a> {
    pub path: Option<&'a gaanim_scene::Path2D>,
    pub source: Option<&'a gaanim_scene::PathSource>,
    pub fill: Option<&'a gaanim_scene::FillBrush>,
    pub stroke: Option<&'a StrokeBrush>,
    pub raster: Option<&'a RasterImage>,
    pub lottie: bool,
    pub shadow: Option<&'a DropShadow>,
    pub glow: Option<&'a Glow>,
    pub blur: Option<&'a GaussianBlur>,
    pub fill_progress: Option<&'a gaanim_animation::FillDrawProgress>,
    pub tip_glow: Option<&'a gaanim_animation::WriteTipGlow>,
    pub stroke_align: Option<&'a StrokeAlign>,
    pub stroke_profile: Option<&'a StrokeProfile>,
    pub stroke_view: Option<kurbo::Affine>,
    pub screen: bool,
    pub chalk: Option<&'a ChalkBrush>,
}

/// The recipe of a drawable's components.
/// A blurred drop shadow of a filled vector region, cast by a member of a
/// group. The composition draws it once for a run of members that share
/// it, from the union of their outlines, instead of once per member: one
/// blur instead of a pair of layers per glyph of a text, and one silhouette
/// whose overlaps do not darken.
#[derive(Debug, Clone, PartialEq)]
pub struct GroupShadow {
    pub shadow: DropShadow,
    /// Outline casting the shadow, in the member's local coordinates.
    pub path: Arc<kurbo::BezPath>,
}

/// The [`GroupShadow`] of a fragment with these parts, when `shareable` (a
/// group member drawn plainly: no clip, blend or screen) and its shadow is
/// a blurred one cast by a visible filled outline.
pub(crate) fn group_shadow(parts: &FragmentParts<'_>, shareable: bool) -> Option<GroupShadow> {
    let shadow = parts
        .shadow
        .filter(|shadow| shareable && shadow.blur_radius.is_finite() && shadow.blur_radius > 0.0)?;
    if parts.raster.is_some_and(|raster| raster.image.is_some())
        || parts.lottie
        || parts.fill.and_then(|fill| fill.0.as_ref()).is_none()
    {
        return None;
    }
    if parts
        .tip_glow
        .is_some_and(|tip| tip.completion <= f64::EPSILON)
    {
        return None;
    }
    let path = &parts.path?.0;
    if path.elements().is_empty() {
        return None;
    }
    // A closed outline being drawn casts the shadow of its stroke.
    let trimmed_closed = parts.source.is_some_and(|source| {
        *source.0 != **path && source.0.elements().contains(&kurbo::PathEl::ClosePath)
    });
    (!trimmed_closed).then(|| GroupShadow {
        shadow: shadow.clone(),
        path: Arc::clone(path),
    })
}

pub(crate) fn fragment_recipe(parts: FragmentParts<'_>) -> FragmentRecipe {
    FragmentRecipe {
        path: parts.path.map(|path| Arc::clone(&path.0)),
        source: parts.source.map(|source| Arc::clone(&source.0)),
        fill: parts.fill.and_then(|fill| fill.0.clone()),
        stroke: parts.stroke.cloned(),
        raster: parts.raster.cloned(),
        lottie: parts.lottie,
        shadow: parts.shadow.cloned(),
        glow: parts.glow.cloned(),
        blur: parts.blur.copied(),
        fill_progress: parts.fill_progress.map(|progress| progress.0),
        completion: parts.tip_glow.map(|tip| tip.completion),
        stroke_align: parts.stroke_align.copied().unwrap_or_default(),
        stroke_profile: parts.stroke_profile.cloned(),
        stroke_view: parts.stroke_view,
        screen: parts.screen,
        chalk: parts.chalk.copied(),
    }
}

/// A built fragment: its commands, the stroke of a camera view screen, and
/// the layers that fill its soft shadow and glow images.
pub struct BuiltFragment {
    pub scene: vello::Scene,
    pub overlay: Option<vello::Scene>,
    pub soft: Vec<crate::object_effects::EffectLayer>,
}

impl FragmentRecipe {
    /// The outline drawn: empty while a write reveal has not started.
    fn visible_path<'a>(&'a self, empty: &'a kurbo::BezPath) -> &'a kurbo::BezPath {
        if self
            .completion
            .is_some_and(|completion| completion <= f64::EPSILON)
        {
            empty
        } else {
            self.path.as_deref().unwrap_or(empty)
        }
    }
}

/// Build the fragment a recipe describes. `lottie` is the Lottie scene of a
/// recipe with [`FragmentRecipe::lottie`] set.
pub fn build_fragment(recipe: &FragmentRecipe, lottie: Option<&vello::Scene>) -> BuiltFragment {
    build_fragment_with(recipe, lottie, false)
}

/// [`build_fragment`]; with `gpu`, a blurred drop shadow and a glow are
/// images that [`BuiltFragment::soft`] fills on the GPU, for a renderer that
/// runs object effects, instead of stacks of vector copies.
pub fn build_fragment_with(
    recipe: &FragmentRecipe,
    lottie: Option<&vello::Scene>,
    gpu: bool,
) -> BuiltFragment {
    use crate::pipeline::{
        ShadowCaster, animated_stroke_paint, draw_aligned_stroke, draw_blur_on_gpu, draw_glow,
        draw_shadow, draw_shadow_on_gpu, draw_soft_fill, draw_soft_stroke, modulate_brush_alpha,
    };

    let mut soft = Vec::new();
    let mut scene = vello::Scene::new();
    if let Some(lottie) = lottie {
        scene.append(lottie, None);
    }

    let empty = kurbo::BezPath::new();
    let elem_path = recipe.visible_path(&empty);
    let source_path = recipe.source.as_deref();
    let is_trimmed_closed = source_path
        .is_some_and(|src| src != elem_path && src.elements().contains(&kurbo::PathEl::ClosePath));
    // Chalk trembles the outline it draws, measured in scene units: the
    // stroke correction maps local lengths to scene lengths.
    let chalk_unit = recipe.stroke_view.map_or(1.0, |view| {
        let determinant = view.determinant().abs();
        if determinant > 1.0e-12 {
            1.0 / determinant.sqrt()
        } else {
            1.0
        }
    });
    let rough = recipe.chalk.as_ref().map(|chalk| {
        (
            crate::chalk::roughen(elem_path, chalk, chalk_unit),
            source_path.map(|source| crate::chalk::roughen(source, chalk, chalk_unit)),
        )
    });
    let (elem_path, source_path) = match &rough {
        Some((path, source)) => (path, source.as_ref()),
        None => (elem_path, source_path),
    };
    let elem_fill = recipe.fill.as_ref();
    let elem_stroke = recipe
        .stroke
        .as_ref()
        .and_then(|stroke| stroke.brush.as_ref());
    let elem_stroke_style = recipe.stroke.as_ref().map(|stroke| &stroke.style);
    let stroke_view = recipe.stroke_view;
    let fill_alpha = recipe
        .fill_progress
        .map(|progress| progress.clamp(0.0, 1.0))
        .unwrap_or(1.0);
    let completion_alpha = recipe.completion.unwrap_or(1.0);
    let anim_wave = if fill_alpha > 0.0 && fill_alpha < 1.0 {
        (fill_alpha as f64 * std::f64::consts::PI).sin()
    } else if completion_alpha > 0.0 && completion_alpha < 1.0 {
        (completion_alpha * std::f64::consts::PI).sin()
    } else {
        0.0
    };

    // 1. Drop shadow and glow, under the geometry. The shadow follows what
    // is painted: the filled region, or the stroke of an unfilled path.
    if let Some(shadow) = &recipe.shadow {
        let filled = (elem_fill.is_some() && !is_trimmed_closed)
            || recipe
                .raster
                .as_ref()
                .is_some_and(|raster| raster.image.is_some());
        let caster = if filled {
            ShadowCaster::Fill
        } else if let (Some(_), Some(style)) = (elem_stroke, elem_stroke_style) {
            ShadowCaster::Stroke {
                style,
                view: stroke_view,
                source: source_path,
                align: recipe.stroke_align,
                profile: recipe.stroke_profile.as_ref(),
            }
        } else {
            ShadowCaster::None
        };
        match gpu
            .then(|| draw_shadow_on_gpu(&mut scene, elem_path, shadow, caster))
            .flatten()
        {
            Some(layer) => soft.push(layer),
            None => draw_shadow(&mut scene, elem_path, shadow, caster),
        }
    }
    if let Some(glow) = &recipe.glow {
        match gpu
            .then(|| crate::soft_effects::draw_glow_image(&mut scene, elem_path, glow, stroke_view))
            .flatten()
        {
            Some(layer) => soft.push(layer),
            None => draw_glow(&mut scene, elem_path, glow, stroke_view),
        }
    }
    let blur_sigma = recipe
        .blur
        .map(|blur| blur.sigma)
        .filter(|sigma| sigma.is_finite() && *sigma > 0.0);
    let blurred_vector = if let Some(sigma) = blur_sigma
        && gpu
        && let Some(layer) = draw_blur_on_gpu(
            &mut scene,
            elem_path,
            elem_fill.filter(|_| !is_trimmed_closed),
            elem_stroke.zip(elem_stroke_style),
            stroke_view,
            sigma,
            fill_alpha,
        ) {
        soft.push(layer);
        true
    } else if let Some(sigma) = blur_sigma {
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

    // Chalk paints the fill and stroke in a layer the grain then masks.
    let chalk_bounds = recipe
        .chalk
        .filter(|_| !blurred_vector && !elem_path.is_empty())
        .map(|chalk| {
            use kurbo::Shape;
            let pen = elem_stroke_style.map_or(0.0, |style| style.width.abs());
            let reach = pen * chalk_unit.max(1.0) + (chalk.roughness * 2.0 + 0.05) * chalk_unit;
            let reach = if reach.is_finite() { reach } else { chalk_unit };
            (chalk, elem_path.bounding_box().inflate(reach, reach))
        });
    if let Some((_, bounds)) = chalk_bounds {
        scene.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            kurbo::Affine::IDENTITY,
            &bounds,
        );
    }

    // 2. Fill, or the raster content clipped to the outline.
    if !blurred_vector
        && let Some(raster_image) = &recipe.raster
        && let Some(image) = raster_image.image.as_ref()
    {
        scene.push_clip_layer(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, elem_path);
        scene.draw_image(image.as_ref(), raster_image.local_transform);
        scene.pop_layer();
    } else if !blurred_vector && fill_alpha < 1.0 {
        if let Some(fill_brush) = elem_fill
            && fill_alpha > 0.0
            && !is_trimmed_closed
        {
            // Clip so the fading fill stays strictly inside the contour.
            scene.push_clip_layer(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, elem_path);
            // Fade the authored paint through alpha only; a white
            // illumination pass made the fill appear abruptly.
            if let Some(brush) = modulate_brush_alpha(fill_brush, fill_alpha) {
                scene.fill(
                    peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    &brush,
                    None,
                    elem_path,
                );
            }
            scene.pop_layer();
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

    // 3. Stroke. A screen draws it above what its camera sees.
    let mut overlay = recipe.screen.then(vello::Scene::new);
    if !blurred_vector
        && let Some(stroke_brush) = elem_stroke
        && let Some(style) = elem_stroke_style
    {
        let (effective_stroke_brush, effective_style) =
            animated_stroke_paint(stroke_brush, style, anim_wave);
        draw_aligned_stroke(
            overlay.as_mut().unwrap_or(&mut scene),
            &effective_style,
            &effective_stroke_brush,
            stroke_view,
            elem_path,
            source_path,
            recipe.stroke_align,
            recipe.stroke_profile.as_ref(),
        );
    }
    if let Some((chalk, bounds)) = chalk_bounds {
        crate::chalk::mask_with_grain(&mut scene, &chalk, chalk_unit, bounds);
        scene.pop_layer();
    }

    BuiltFragment {
        scene,
        overlay,
        soft,
    }
}

/// Built fragments of recorded recipes, shared by every frame that draws
/// the same recipe (and, for a Lottie, the same Lottie frame).
#[derive(Default)]
pub struct FragmentStore {
    built: std::collections::HashMap<(usize, usize, bool), StoredFragment>,
    /// Frames composed so far, for evicting fragments no longer drawn.
    generation: u64,
}

struct StoredFragment {
    recipe: Arc<FragmentRecipe>,
    lottie: Option<Arc<vello::Scene>>,
    scene: Arc<vello::Scene>,
    overlay: Option<Arc<vello::Scene>>,
    soft: Arc<[crate::object_effects::EffectLayer]>,
    last_used: u64,
}

/// Frames a fragment stays built after it was last drawn.
const FRAGMENT_KEEP_FRAMES: u64 = 240;

/// Frames between eviction sweeps. A sweep visits every stored fragment, and
/// a scene with thousands of them would pay that on every frame. The store
/// holds each recipe it keys by address, so a late sweep never confuses a
/// freed recipe with a new one.
const FRAGMENT_SWEEP_FRAMES: u64 = 60;

impl FragmentStore {
    /// The fragment and screen overlay of `recipe`, built once per shared
    /// recipe allocation.
    pub fn get(
        &mut self,
        recipe: &Arc<FragmentRecipe>,
        lottie: Option<&Arc<vello::Scene>>,
    ) -> (Arc<vello::Scene>, Option<Arc<vello::Scene>>) {
        let (scene, overlay, _) = self.get_with(recipe, lottie, false);
        (scene, overlay)
    }

    /// [`Self::get`] built with [`build_fragment_with`], and the layers of
    /// its soft effects.
    pub fn get_with(
        &mut self,
        recipe: &Arc<FragmentRecipe>,
        lottie: Option<&Arc<vello::Scene>>,
        gpu: bool,
    ) -> (
        Arc<vello::Scene>,
        Option<Arc<vello::Scene>>,
        Arc<[crate::object_effects::EffectLayer]>,
    ) {
        let key = (
            Arc::as_ptr(recipe) as usize,
            lottie.map_or(0, |scene| Arc::as_ptr(scene) as usize),
            gpu,
        );
        let generation = self.generation;
        let entry = self.built.entry(key).or_insert_with(|| {
            let built = build_fragment_with(recipe, lottie.map(Arc::as_ref), gpu);
            StoredFragment {
                recipe: Arc::clone(recipe),
                lottie: lottie.cloned(),
                scene: Arc::new(built.scene),
                overlay: built.overlay.map(Arc::new),
                soft: built.soft.into(),
                last_used: generation,
            }
        });
        entry.last_used = generation;
        (
            Arc::clone(&entry.scene),
            entry.overlay.clone(),
            Arc::clone(&entry.soft),
        )
    }

    /// Forget fragments whose recipes (or Lottie scenes) nothing else holds.
    pub fn retain_shared(&mut self) {
        self.built.retain(|_, stored| {
            Arc::strong_count(&stored.recipe) > 1
                && stored
                    .lottie
                    .as_ref()
                    .is_none_or(|scene| Arc::strong_count(scene) > 1)
        });
    }

    /// Finish a frame: every [`FRAGMENT_SWEEP_FRAMES`], forget fragments not
    /// drawn for a while, and those whose recipes nothing else holds.
    pub fn end_frame(&mut self) {
        self.generation += 1;
        if !self.generation.is_multiple_of(FRAGMENT_SWEEP_FRAMES) {
            return;
        }
        let oldest = self.generation.saturating_sub(FRAGMENT_KEEP_FRAMES);
        self.built.retain(|_, stored| {
            stored.last_used >= oldest
                && Arc::strong_count(&stored.recipe) > 1
                && stored
                    .lottie
                    .as_ref()
                    .is_none_or(|scene| Arc::strong_count(scene) > 1)
        });
    }

    pub fn len(&self) -> usize {
        self.built.len()
    }

    pub fn is_empty(&self) -> bool {
        self.built.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_polyline(fill: Option<peniko::Brush>, blur_radius: f64) -> FragmentRecipe {
        FragmentRecipe {
            path: Some(Arc::new(
                kurbo::BezPath::from_svg("M -3 -1 L 0 2 L 3 -1").unwrap(),
            )),
            fill,
            stroke: Some(StrokeBrush {
                brush: Some(peniko::Brush::Solid(peniko::Color::WHITE)),
                style: kurbo::Stroke::new(0.04),
            }),
            shadow: Some(DropShadow {
                offset: gaanim_core::glam::DVec2::new(0.03, -0.03),
                blur_radius,
                color: peniko::Color::from_rgba8(0, 0, 0, 176),
            }),
            stroke_align: StrokeAlign::Center,
            ..Default::default()
        }
    }

    #[test]
    fn unfilled_open_path_casts_the_shadow_of_its_stroke() {
        let recipe = open_polyline(None, 0.0);
        let path = recipe.path.as_deref().unwrap();
        let style = &recipe.stroke.as_ref().unwrap().style;
        let offset = kurbo::Affine::translate((0.03, -0.03));
        let shadow = peniko::Brush::Solid(recipe.shadow.as_ref().unwrap().color);
        let mut expected = vello::Scene::new();
        expected.stroke(
            style,
            kurbo::Affine::IDENTITY,
            &shadow,
            None,
            &(offset * path),
        );
        expected.stroke(
            style,
            kurbo::Affine::IDENTITY,
            recipe.stroke.as_ref().unwrap().brush.as_ref().unwrap(),
            None,
            path,
        );
        let built = build_fragment(&recipe, None).scene;
        assert_eq!(built.encoding().path_data, expected.encoding().path_data);
        assert!(built.encoding().draw_tags == expected.encoding().draw_tags);
    }

    #[test]
    fn filled_path_keeps_the_shadow_of_its_region() {
        let fill = peniko::Brush::Solid(peniko::Color::WHITE);
        let recipe = open_polyline(Some(fill.clone()), 0.0);
        let path = recipe.path.as_deref().unwrap();
        let offset = kurbo::Affine::translate((0.03, -0.03));
        let shadow = peniko::Brush::Solid(recipe.shadow.as_ref().unwrap().color);
        let mut expected = vello::Scene::new();
        expected.fill(peniko::Fill::NonZero, offset, &shadow, None, path);
        let built = build_fragment(&recipe, None).scene;
        let shadow_len = expected.encoding().path_data.len();
        assert_eq!(
            built.encoding().path_data[..shadow_len],
            expected.encoding().path_data[..]
        );
    }
}
