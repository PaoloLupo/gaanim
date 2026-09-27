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

use crate::effects::{DropShadow, GaussianBlur, Glow, StrokeAlign, StrokeProfile};

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
}

/// The recipe of a drawable's components.
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
    }
}

/// A built fragment: its commands, and the stroke of a camera view screen.
pub struct BuiltFragment {
    pub scene: vello::Scene,
    pub overlay: Option<vello::Scene>,
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
    use crate::pipeline::{
        animated_stroke_paint, draw_aligned_stroke, draw_glow, draw_shadow, draw_soft_fill,
        draw_soft_stroke, modulate_brush_alpha,
    };

    let mut scene = vello::Scene::new();
    if let Some(lottie) = lottie {
        scene.append(lottie, None);
    }

    let empty = kurbo::BezPath::new();
    let elem_path = recipe.visible_path(&empty);
    let source_path = recipe.source.as_deref();
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

    // 1. Drop shadow and glow, under the geometry.
    if let Some(shadow) = &recipe.shadow {
        draw_shadow(&mut scene, elem_path, shadow);
    }
    if let Some(glow) = &recipe.glow {
        draw_glow(&mut scene, elem_path, glow, stroke_view);
    }

    let is_trimmed_closed = source_path
        .is_some_and(|src| src != elem_path && src.elements().contains(&kurbo::PathEl::ClosePath));
    let blur_sigma = recipe
        .blur
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

    BuiltFragment { scene, overlay }
}

/// Built fragments of recorded recipes, shared by every frame that draws
/// the same recipe (and, for a Lottie, the same Lottie frame).
#[derive(Default)]
pub struct FragmentStore {
    built: std::collections::HashMap<(usize, usize), StoredFragment>,
}

struct StoredFragment {
    recipe: Arc<FragmentRecipe>,
    lottie: Option<Arc<vello::Scene>>,
    scene: Arc<vello::Scene>,
    overlay: Option<Arc<vello::Scene>>,
}

impl FragmentStore {
    /// The fragment and screen overlay of `recipe`, built once per shared
    /// recipe allocation.
    pub fn get(
        &mut self,
        recipe: &Arc<FragmentRecipe>,
        lottie: Option<&Arc<vello::Scene>>,
    ) -> (Arc<vello::Scene>, Option<Arc<vello::Scene>>) {
        let key = (
            Arc::as_ptr(recipe) as usize,
            lottie.map_or(0, |scene| Arc::as_ptr(scene) as usize),
        );
        let entry = self.built.entry(key).or_insert_with(|| {
            let built = build_fragment(recipe, lottie.map(Arc::as_ref));
            StoredFragment {
                recipe: Arc::clone(recipe),
                lottie: lottie.cloned(),
                scene: Arc::new(built.scene),
                overlay: built.overlay.map(Arc::new),
            }
        });
        (Arc::clone(&entry.scene), entry.overlay.clone())
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

    pub fn len(&self) -> usize {
        self.built.len()
    }

    pub fn is_empty(&self) -> bool {
        self.built.is_empty()
    }
}
