//! 2D drawables placed in the 3D world ([`InWorld3D`]). Each frame the
//! outlines their parts draw, filled and stroked as the 2D renderer would
//! draw them, become the drawable's [`PlaneFillData`], which the 3D pass
//! fills from their curves (see [`crate::plane_fill`]); the 2D renderer
//! leaves the parts, marked [`DrawnIn3D`], to it.
//!
//! A part's fill and stroke keep their colors and gradients, its opacity and
//! the progress of a write, and a stroke its alignment and profile; an
//! image fills its outline as the 2D renderer clips it there, and a Lottie
//! scene becomes the outlines it fills and strokes (see [`LottieOutlines`]).
//! A part's glow, drop shadow or blur becomes the drawable's
//! ([`PartEffects`]), which the 3D pass draws; chalk is left out.
//!
//! Billboards are drawn this way too, facing the camera (see
//! [`billboards_in_world_system`]), unless they hold what is left out.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use bevy::prelude::*;
use gaanim_animation::{FillDrawProgress, WriteTipGlow};
use gaanim_core::kurbo::{self, Affine, BezPath, Shape};
use gaanim_core::peniko;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::{
    Billboard, BillboardPlane, DrawnIn3D, FillBrush, GlobalOpacity, GroupMarker, HudOverlay,
    InWorld3D, Path2D, PathSource, PlaneClip, PlaneFillData, PlaneGlyph, PlaneImage, RasterImage,
    RenderLayer, StrokeBrush, Visible,
};

use crate::effects::{
    ChalkBrush, ClipMask, DropShadow, GaussianBlur, Glow, StrokeAlign, StrokeProfile,
    StrokeScalesWithObject,
};
use crate::fragment::{FragmentParts, FragmentRecipe, fragment_recipe};
use crate::lottie::{LottieCanvas, LottiePlayer};

/// What a part draws at full opacity.
type Drawn = Arc<[Laid]>;

/// An outline a part draws, laid at its box's corner, with that corner, its
/// paint, and the contour that clips it, laid at its own corner, with that
/// corner and whether it keeps what lies outside.
#[derive(Clone, Debug, PartialEq)]
struct Laid {
    outline: Arc<BezPath>,
    corner: kurbo::Vec2,
    paint: Paint,
    clip: Option<(Arc<BezPath>, kurbo::Vec2, bool)>,
}

/// What an outline is filled with: a color, a gradient or an image, with
/// the map from the part's own units to the gradient's units or the image's
/// pixels, as the 2D renderer paints it.
#[derive(Clone, Debug, PartialEq)]
enum Paint {
    Solid(peniko::Color),
    Gradient(Arc<peniko::Gradient>, Affine),
    Image(Arc<peniko::ImageBrush>, Affine),
}

impl Paint {
    /// The paint `brush` fills with; none for an image or a gradient
    /// without stops.
    fn of(brush: &peniko::Brush) -> Option<Self> {
        match brush {
            peniko::Brush::Solid(color) => Some(Self::Solid(*color)),
            peniko::Brush::Gradient(gradient) if !gradient.stops.is_empty() => {
                Some(Self::Gradient(Arc::new(gradient.clone()), Affine::IDENTITY))
            }
            peniko::Brush::Gradient(_) | peniko::Brush::Image(_) => None,
        }
    }

    fn multiply_alpha(self, alpha: f32) -> Self {
        if alpha >= 1.0 {
            return self;
        }
        match self {
            Self::Solid(color) => Self::Solid(color.multiply_alpha(alpha)),
            Self::Gradient(gradient, to_gradient) => Self::Gradient(
                Arc::new(gradient.as_ref().clone().multiply_alpha(alpha)),
                to_gradient,
            ),
            Self::Image(brush, to_pixels) => {
                let mut brush = brush.as_ref().clone();
                brush.sampler.alpha *= alpha;
                Self::Image(Arc::new(brush), to_pixels)
            }
        }
    }

    /// The color, gradient and image of a glyph of an outline laid at
    /// `corner`, at `alpha`. An image's alpha goes into the color, which
    /// multiplies its texels'.
    fn glyph(&self, alpha: f32, corner: kurbo::Vec2) -> GlyphPaint {
        match self.clone().multiply_alpha(alpha) {
            Self::Solid(color) => (color, None, None),
            Self::Gradient(gradient, to_gradient) => (
                gradient.stops[0].color.to_alpha_color(),
                Some((gradient, to_gradient * Affine::translate(corner))),
                None,
            ),
            Self::Image(brush, to_pixels) => {
                let mut brush = brush.as_ref().clone();
                let alpha = std::mem::replace(&mut brush.sampler.alpha, 1.0);
                (
                    peniko::Color::WHITE.multiply_alpha(alpha),
                    None,
                    Some(PlaneImage {
                        brush,
                        to_pixels: to_pixels * Affine::translate(corner),
                    }),
                )
            }
        }
    }
}

/// A glyph's color, gradient and the map to it, and image.
type GlyphPaint = (
    peniko::Color,
    Option<(Arc<peniko::Gradient>, Affine)>,
    Option<PlaneImage>,
);

/// The outlines parts drew, by part, while their recipe stays the same, and
/// outlines alike, shared: the same letter again packs and records once.
#[derive(Default)]
pub(crate) struct InWorldParts {
    parts: HashMap<Entity, (FragmentRecipe, Drawn)>,
    /// What Lottie scenes drew, by part, with the instant they showed.
    lotties: HashMap<Entity, (LottieShown, Drawn)>,
    /// Outlines laid at their box's corner, by their size and their points on
    /// a grid a 65536th of their size.
    alike: HashMap<Vec<i64>, Weak<BezPath>>,
    /// Outlines alike the last sweep of dead ones left.
    swept: usize,
}

/// Components of a part of an [`InWorld3D`] drawable.
type PartData = (
    Has<DrawnIn3D>,
    Has<Visible>,
    Has<GroupMarker>,
    &'static GlobalSpatialTransform,
    &'static GlobalOpacity,
    &'static RenderLayer,
    Option<&'static Path2D>,
    Option<&'static PathSource>,
    Option<&'static FillBrush>,
    Option<&'static StrokeBrush>,
    Option<&'static FillDrawProgress>,
    Option<&'static WriteTipGlow>,
    Option<&'static StrokeAlign>,
    Option<&'static StrokeProfile>,
    (
        Has<StrokeScalesWithObject>,
        Option<&'static RasterImage>,
        Option<&'static LottiePlayer>,
        Option<&'static Glow>,
        Option<&'static DropShadow>,
        Option<&'static GaussianBlur>,
    ),
);

/// The glow, drop shadow and blur the parts of a drawable in the 3D world
/// hold, as a text's letters hold the text's: the 3D pass draws them for the
/// drawable when it has none of its own (see
/// `crate::pipeline::three_d_elements`).
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub struct PartEffects {
    pub glow: Option<Glow>,
    pub shadow: Option<DropShadow>,
    pub blur: Option<GaussianBlur>,
}

/// What a part may hold that the 3D pass leaves out: chalk.
type LeftOut = With<ChalkBrush>;

/// A billboard that holds what the 3D pass leaves out, which stays 2D
/// content drawn over the 3D image.
#[derive(Component)]
pub(crate) struct BillboardOver3D;

/// Hand each billboard to the 3D pass, as an [`InWorld3D`] drawable that
/// faces the camera (`billboard_system` keeps its [`BillboardPlane`]), so that the meshes and
/// lines in front of it hide it pixel by pixel. One whose parts hold what
/// the 3D pass leaves out ([`LeftOut`]) stays 2D content over the 3D
/// image, and fades as a whole behind meshes
/// ([`gaanim_scene::occlusion`]). Each is decided once, when first seen,
/// before it is placed.
pub(crate) fn billboards_in_world_system(
    mut commands: Commands,
    billboards: Query<
        Entity,
        (
            With<Billboard>,
            Without<InWorld3D>,
            Without<BillboardOver3D>,
            Without<HudOverlay>,
        ),
    >,
    children: Query<&Children>,
    left_out: Query<(), LeftOut>,
) {
    for billboard in &billboards {
        let mut stack = vec![billboard];
        let mut drawable = true;
        while let Some(entity) = stack.pop() {
            if left_out.contains(entity) {
                drawable = false;
                break;
            }
            if let Ok(kids) = children.get(entity) {
                stack.extend(kids.iter());
            }
        }
        if drawable {
            commands
                .entity(billboard)
                .insert((InWorld3D::default(), BillboardPlane::default()))
                .remove::<gaanim_scene::occlusion::Occlusion>();
        } else {
            commands.entity(billboard).insert(BillboardOver3D);
        }
    }
}

/// Keep the [`PlaneFillData`] of each [`InWorld3D`] drawable: the outlines
/// its visible parts draw, placed in its plane relative to it, at their
/// opacity within it, and clipped by their [`ClipMask`], as a wipe reveals
/// them. A drawable whose outlines did not change keeps its
/// data untouched, so its change tick, by which the 3D pass and recordings
/// keep their uploads, stays.
pub(crate) fn sync_in_world_system(
    mut commands: Commands,
    roots: Query<
        (
            Entity,
            &GlobalSpatialTransform,
            &GlobalOpacity,
            Option<&PlaneFillData>,
            Option<&PartEffects>,
        ),
        With<InWorld3D>,
    >,
    children: Query<&Children>,
    parts: Query<PartData>,
    masks: Query<&ClipMask>,
    mut held: Local<InWorldParts>,
) {
    let held = &mut *held;
    let mut seen = HashSet::new();
    for (root, root_transform, root_opacity, current, current_effects) in &roots {
        let mut effects = PartEffects::default();
        let to_root = root_transform.affine_2d;
        if to_root.determinant().abs() <= 1e-12 {
            continue;
        }
        let to_root = to_root.inverse();
        let root_opacity = root_opacity.0.max(f32::EPSILON);
        let mut glyphs = Vec::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            if let Ok(kids) = children.get(entity) {
                stack.extend(kids.iter().rev());
            }
            let Ok((
                marked,
                visible,
                group,
                transform,
                opacity,
                layer,
                path,
                source,
                fill,
                stroke,
                fill_progress,
                tip_glow,
                stroke_align,
                stroke_profile,
                (stroke_scales, raster, lottie, glow, shadow, blur),
            )) = parts.get(entity)
            else {
                continue;
            };
            // The first glow and shadow among the parts, the root's aside,
            // which the 3D pass finds on the root itself.
            if entity != root {
                if effects.glow.is_none() {
                    effects.glow = glow.cloned();
                }
                if effects.shadow.is_none() {
                    effects.shadow = shadow.cloned();
                }
                if effects.blur.is_none() {
                    effects.blur = blur.copied();
                }
            }
            if group || *layer != RenderLayer::Vello2D || path.is_none() {
                continue;
            }
            if !marked {
                commands.entity(entity).insert(DrawnIn3D);
            }
            seen.insert(entity);
            if !visible || opacity.0 <= f32::EPSILON {
                continue;
            }
            let stroked = stroke.is_some_and(|stroke| stroke.brush.is_some());
            let recipe = fragment_recipe(FragmentParts {
                path,
                source,
                fill,
                stroke,
                raster: raster.filter(|raster| raster.image.is_some()),
                lottie: false,
                shadow: None,
                glow: None,
                blur: None,
                fill_progress,
                tip_glow,
                stroke_align,
                stroke_profile,
                stroke_view: (stroked && !stroke_scales)
                    .then(|| crate::stroke::scene_unit_stroke_transform(transform.affine_2d))
                    .flatten(),
                screen: false,
                chalk: None,
            });
            let drawn = match held.parts.get(&entity) {
                Some((kept, drawn)) if *kept == recipe => Arc::clone(drawn),
                _ => {
                    let drawn = drawn(&recipe, &mut held.alike);
                    held.parts.insert(entity, (recipe, Arc::clone(&drawn)));
                    drawn
                }
            };
            // A Lottie scene draws beneath the part's own outline, as in 2D,
            // drawn again only when it shows another instant.
            let animated = lottie.map(|player| {
                let shown = player.shown();
                match held.lotties.get(&entity) {
                    Some((kept, drawn)) if *kept == shown => Arc::clone(drawn),
                    _ => {
                        let drawn = lottie_drawn(player, &mut held.alike);
                        held.lotties.insert(entity, (shown, Arc::clone(&drawn)));
                        drawn
                    }
                }
            });
            // The part's mask, in its own units as its outlines are. A mask
            // that encloses nothing keeps nothing, or all of it inverted.
            let mask = match masks.get(entity).map(ClipMask::kept_contour) {
                Ok((contour, outside)) => match lay(contour, &mut held.alike) {
                    Some((contour, corner)) => Some((contour, corner, outside)),
                    None if outside => None,
                    None => continue,
                },
                Err(_) => None,
            };
            let placement = to_root * transform.affine_2d;
            let alpha = (opacity.0 / root_opacity).clamp(0.0, 1.0);
            for laid in animated
                .iter()
                .flat_map(|drawn| drawn.iter())
                .chain(drawn.iter())
            {
                let (color, gradient, image) = laid.paint.glyph(alpha, laid.corner);
                // A glyph takes one contour: a stroke aligned inside or
                // outside its shape keeps the one that aligns it.
                let clip = laid.clip.as_ref().or(mask.as_ref());
                glyphs.push(PlaneGlyph {
                    outline: Arc::clone(&laid.outline),
                    placement: placement * Affine::translate(laid.corner),
                    color,
                    gradient,
                    clip: clip.map(|(outline, corner, outside)| PlaneClip {
                        outline: Arc::clone(outline),
                        offset: laid.corner - *corner,
                        outside: *outside,
                    }),
                    image,
                });
            }
        }
        if current.is_none_or(|current| !same_glyphs(&current.glyphs, &glyphs)) {
            commands.entity(root).insert(PlaneFillData { glyphs });
        }
        if current_effects != Some(&effects) {
            if effects == PartEffects::default() {
                if current_effects.is_some() {
                    commands.entity(root).remove::<PartEffects>();
                }
            } else {
                commands.entity(root).insert(effects);
            }
        }
    }
    held.parts.retain(|entity, _| seen.contains(entity));
    held.lotties.retain(|entity, _| seen.contains(entity));
    if held.alike.len() >= 64 && held.alike.len() >= 2 * held.swept {
        held.alike.retain(|_, outline| outline.strong_count() > 0);
        held.swept = held.alike.len();
    }
}

/// System: the mask of a wipe on a drawable placed in the 3D world sweeps
/// across the drawable in its own plane. A wipe's mask is a 2D drawable
/// that follows a point between two anchors of the wiped drawable, turned
/// toward the travel in the scene's plane, and [`crate::pipeline::
/// resolve_dynamic_clip_masks_system`] brings it into each part through the
/// parts' projection on that plane, which a drawable turned out of it (its
/// x axis along z, say) flattens. Here that point is found in the wiped
/// drawable's own units, the mask is turned there, and each part takes it
/// through its transform relative to the wiped drawable, whatever way the
/// drawable faces. Before the wipe's cursor the mask waits off the drawable,
/// which keeps nothing of it.
pub(crate) fn resolve_in_world_wipes_system(world: &mut World) {
    use gaanim_animation::{EndpointFollow, TrackingEndpoint};

    let time = world
        .get_resource::<gaanim_animation::PlaybackState>()
        .map_or(0.0, |state| state.current_time);
    let mut targets = world.query::<(Entity, &ClipMask)>();
    let candidates: Vec<(Entity, Entity)> = targets
        .iter(world)
        .filter(|(_, mask)| !mask.invert && mask.sources.len() == 1)
        .map(|(target, mask)| (target, mask.sources[0]))
        .collect();
    let mut updates = Vec::new();
    for (target, mask) in candidates {
        let in_world = std::iter::successors(Some(target), |node| {
            world.get::<ChildOf>(*node).map(ChildOf::parent)
        })
        .any(|node| world.get::<InWorld3D>(node).is_some());
        if !in_world {
            continue;
        }
        let Some(follow) = world.get::<EndpointFollow>(mask) else {
            continue;
        };
        let TrackingEndpoint::Between {
            from,
            to,
            alpha,
            offset,
        } = &follow.endpoint
        else {
            continue;
        };
        let (
            TrackingEndpoint::EntityAnchor {
                entity: wiped,
                normalized: start,
                offset: start_offset,
            },
            TrackingEndpoint::EntityAnchor {
                entity: wiped_too,
                normalized: end,
                offset: end_offset,
            },
        ) = (from.as_ref(), to.as_ref())
        else {
            continue;
        };
        if wiped != wiped_too {
            continue;
        }
        if !follow.holds_at(time) {
            updates.push((target, BezPath::new()));
            continue;
        }
        let (Some(bounds), Some(alpha), Some(outline), Some(turn)) = (
            world.get::<gaanim_scene::LocalBounds>(*wiped),
            alpha.evaluate(world),
            world.get::<Path2D>(mask),
            world.get::<gaanim_math::SpatialTransform>(mask),
        ) else {
            continue;
        };
        let (Some(wiped_world), Some(target_world)) = (
            world.get::<GlobalSpatialTransform>(*wiped),
            world.get::<GlobalSpatialTransform>(target),
        ) else {
            continue;
        };
        // The edge's point in the wiped drawable's units, and the mask
        // around it, turned toward the travel in that plane.
        let (center, half) = (bounds.0.center(), bounds.0.size() * 0.5);
        let from = center + half * *start + *start_offset;
        let to = center + half * *end + *end_offset;
        let at = from.lerp(to, alpha) + *offset;
        let [a, b, c, d, _, _] = turn.to_affine_2d().as_coeffs();
        let in_wiped = Affine::new([a, b, c, d, at.x, at.y]);
        // From the wiped drawable's units to the part's, in their plane.
        let relative = target_world.mat4.inverse() * wiped_world.mat4;
        let to_target = Affine::new([
            relative.x_axis.x,
            relative.x_axis.y,
            relative.y_axis.x,
            relative.y_axis.y,
            relative.w_axis.x,
            relative.w_axis.y,
        ]);
        updates.push((target, (to_target * in_wiped) * (*outline.0).clone()));
    }
    for (target, path) in updates {
        if let Some(mut mask) = world.get_mut::<ClipMask>(target)
            && mask.path != path
        {
            mask.path = path;
        }
    }
}

/// Whether two lists of glyphs draw the same: outlines compared by address
/// first, as unchanged parts keep theirs.
fn same_glyphs(a: &[PlaneGlyph], b: &[PlaneGlyph]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.placement == b.placement
                && a.color == b.color
                && a.gradient == b.gradient
                && a.clip == b.clip
                && a.image == b.image
                && (Arc::ptr_eq(&a.outline, &b.outline) || a.outline == b.outline)
        })
}

/// The outlines `recipe` fills and strokes, as the 2D renderer draws them
/// (see [`crate::fragment::build_fragment`]): its image clipped to the
/// outline in place of the fill, or the fill unless a write is still
/// drawing a closed outline, faded by the write's fill progress; and the
/// stroke as an outline of its own (see [`stroke_outline`]).
fn drawn(recipe: &FragmentRecipe, alike: &mut HashMap<Vec<i64>, Weak<BezPath>>) -> Drawn {
    let empty = BezPath::new();
    let path = recipe.visible_path(&empty);
    let mut drawn = Vec::new();
    if path.elements().is_empty() {
        return drawn.into();
    }
    let trimmed_closed = recipe.source.as_deref().is_some_and(|source| {
        source != path && source.elements().contains(&kurbo::PathEl::ClosePath)
    });
    let fill_alpha = recipe
        .fill_progress
        .map_or(1.0, |progress| progress.clamp(0.0, 1.0));
    let completion = recipe.completion.unwrap_or(1.0);
    let wave = if fill_alpha > 0.0 && fill_alpha < 1.0 {
        (f64::from(fill_alpha) * std::f64::consts::PI).sin()
    } else if completion > 0.0 && completion < 1.0 {
        (completion * std::f64::consts::PI).sin()
    } else {
        0.0
    };
    let image = recipe.raster.as_ref().and_then(|raster| {
        let brush = raster.image.as_ref()?;
        let to_pixels = raster.local_transform;
        (to_pixels.determinant().abs() > 1e-12)
            .then(|| (Arc::new(brush.clone()), to_pixels.inverse()))
    });
    if let Some((brush, to_pixels)) = image {
        drawn.extend(laid(
            path.clone(),
            Paint::Image(brush, to_pixels),
            None,
            alike,
        ));
    } else if !trimmed_closed
        && fill_alpha > 0.0
        && let Some(paint) = recipe.fill.as_ref().and_then(Paint::of)
    {
        drawn.extend(laid(
            path.clone(),
            paint.multiply_alpha(fill_alpha),
            None,
            alike,
        ));
    }
    if let Some(stroke) = &recipe.stroke
        && let Some(brush) = &stroke.brush
    {
        let (brush, style) = crate::pipeline::animated_stroke_paint(brush, &stroke.style, wave);
        let style = crate::stroke::drawable_pen(&style);
        if let Some(paint) = Paint::of(&brush)
            && style.width.is_finite()
            && style.width > 0.0
        {
            let (outline, clip) = stroke_outline(recipe, path, &style);
            let clip = clip.and_then(|(contour, outside)| {
                let (contour, corner) = lay(contour.clone(), alike)?;
                Some((contour, corner, outside))
            });
            drawn.extend(laid(outline, paint, clip, alike));
        }
    }
    drawn.into()
}

/// What tells apart the instants a Lottie scene shows (see
/// [`LottiePlayer::shown`]).
type LottieShown = (u64, usize, Option<u32>, [u64; 3]);

/// How closely the outlines of a Lottie scene follow its curves, in its
/// composition's pixels.
const LOTTIE_TOLERANCE: f64 = 0.05;

/// The outlines a Lottie scene draws, in its part's own units, as the 3D
/// pass fills them: each fill, stroke and image an outline of its own, at
/// the opacity of the layers it lies in, kept within the innermost clip or
/// mask that cuts it. Blend modes draw as the normal one, and a translucent
/// layer fades each of its shapes rather than all of them at once.
#[derive(Default)]
struct LottieOutlines {
    /// The opacity and contour of each layer drawn into.
    layers: Vec<(f32, BezPath)>,
    drawn: Vec<(BezPath, Paint, Option<BezPath>)>,
}

impl LottieOutlines {
    /// Add `outline` painted `paint`, within the layers drawn into.
    fn push(&mut self, outline: BezPath, paint: Paint) {
        let alpha: f32 = self.layers.iter().map(|(alpha, _)| *alpha).product();
        if alpha <= 0.0 || outline.elements().is_empty() {
            return;
        }
        // A rectangle around the outline cuts none of it.
        let bounds = outline.bounding_box();
        let clip = self
            .layers
            .iter()
            .rev()
            .map(|(_, contour)| contour)
            .find(|contour| {
                let around = contour.bounding_box();
                let rectangle =
                    (contour.area().abs() - around.area()).abs() <= around.area() * 1e-9;
                !(rectangle && around.contains_rect(bounds))
            })
            .cloned();
        self.drawn
            .push((outline, paint.multiply_alpha(alpha), clip));
    }

    /// What `brush` paints, drawn by `transform` from its shape's units.
    fn paint(brush: &peniko::Brush, transform: Affine) -> Option<Paint> {
        match brush {
            peniko::Brush::Solid(color) => Some(Paint::Solid(*color)),
            peniko::Brush::Gradient(gradient)
                if !gradient.stops.is_empty() && transform.determinant().abs() > 1e-12 =>
            {
                Some(Paint::Gradient(
                    Arc::new(gradient.clone()),
                    transform.inverse(),
                ))
            }
            _ => None,
        }
    }
}

impl LottieCanvas for LottieOutlines {
    fn push_layer(
        &mut self,
        _blend: peniko::BlendMode,
        alpha: f32,
        transform: Affine,
        shape: &impl Shape,
    ) {
        self.layers
            .push((alpha, transform * shape.to_path(LOTTIE_TOLERANCE)));
    }

    fn push_clip_layer(&mut self, transform: Affine, shape: &impl Shape) {
        self.layers
            .push((1.0, transform * shape.to_path(LOTTIE_TOLERANCE)));
    }

    fn pop_layer(&mut self) {
        self.layers.pop();
    }

    fn fill(&mut self, transform: Affine, brush: &peniko::Brush, shape: &impl Shape) {
        if let Some(paint) = Self::paint(brush, transform) {
            self.push(transform * shape.to_path(LOTTIE_TOLERANCE), paint);
        }
    }

    fn stroke(
        &mut self,
        style: &kurbo::Stroke,
        transform: Affine,
        brush: &peniko::Brush,
        shape: &impl Shape,
    ) {
        if let Some(paint) = Self::paint(brush, transform) {
            let outline = kurbo::stroke(
                shape.path_elements(LOTTIE_TOLERANCE),
                style,
                &kurbo::StrokeOpts::default(),
                LOTTIE_TOLERANCE,
            );
            self.push(transform * outline, paint);
        }
    }

    fn draw_image(&mut self, image: &peniko::ImageBrush, transform: Affine) {
        let (width, height) = (image.image.width, image.image.height);
        if width == 0 || height == 0 || transform.determinant().abs() <= 1e-12 {
            return;
        }
        let rectangle = kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height));
        self.push(
            transform * rectangle.to_path(0.0),
            Paint::Image(Arc::new(image.clone()), transform.inverse()),
        );
    }
}

/// The outlines `player` draws at the instant it shows (see
/// [`LottieOutlines`]).
fn lottie_drawn(player: &LottiePlayer, alike: &mut HashMap<Vec<i64>, Weak<BezPath>>) -> Drawn {
    let mut outlines = LottieOutlines::default();
    player.draw_into(&mut outlines);
    outlines
        .drawn
        .into_iter()
        .filter_map(|(outline, paint, clip)| {
            let clip = clip.and_then(|contour| {
                let (contour, corner) = lay(contour, alike)?;
                Some((contour, corner, false))
            });
            laid(outline, paint, clip, alike)
        })
        .collect::<Vec<_>>()
        .into()
}

/// The outline a stroke of `style` along `path` draws, as
/// [`crate::pipeline::draw_aligned_stroke`] draws it, and the contour that
/// clips it with whether it keeps what lies outside: a profile's outline; for
/// a stroke aligned inside or outside a closed contour, the stroke twice as
/// wide, clipped to that contour; else the stroke widened.
fn stroke_outline<'a>(
    recipe: &'a FragmentRecipe,
    path: &'a BezPath,
    style: &kurbo::Stroke,
) -> (BezPath, Option<(&'a BezPath, bool)>) {
    let in_view = |outline: &dyn Fn(&BezPath) -> BezPath| match recipe.stroke_view {
        Some(view) => view.inverse() * outline(&(view * path.clone())),
        None => outline(path),
    };
    if let Some(profile) = &recipe.stroke_profile {
        let half_width = style.width.abs() * 0.5;
        let outline = in_view(&|path| crate::stroke::profiled_outline(path, half_width, profile));
        return (outline, None);
    }
    let widen = |pen: &kurbo::Stroke| {
        in_view(&|path| {
            let tolerance = (pen.width * 0.01).max(1e-9);
            kurbo::stroke(path.iter(), pen, &kurbo::StrokeOpts::default(), tolerance)
        })
    };
    let clip = match recipe.stroke_align {
        StrokeAlign::Center => None,
        StrokeAlign::Inside | StrokeAlign::Outside => {
            crate::pipeline::stroke_clip_path(path, recipe.source.as_deref())
        }
    };
    match clip {
        Some(contour) => (
            widen(&crate::pipeline::clipped_pen(style)),
            Some((contour, recipe.stroke_align == StrokeAlign::Outside)),
        ),
        None => (widen(style), None),
    }
}

/// `outline` laid at its box's corner, shared with an outline alike, with
/// that corner, `paint` and `clip`; nothing for an empty outline.
fn laid(
    outline: BezPath,
    paint: Paint,
    clip: Option<(Arc<BezPath>, kurbo::Vec2, bool)>,
    alike: &mut HashMap<Vec<i64>, Weak<BezPath>>,
) -> Option<Laid> {
    let (outline, corner) = lay(outline, alike)?;
    Some(Laid {
        outline,
        corner,
        paint,
        clip,
    })
}

/// `outline` laid at its box's corner, shared with an outline alike, and
/// that corner; nothing for an empty outline.
fn lay(
    outline: BezPath,
    alike: &mut HashMap<Vec<i64>, Weak<BezPath>>,
) -> Option<(Arc<BezPath>, kurbo::Vec2)> {
    let bounds = outline.bounding_box();
    let step = bounds.width().max(bounds.height()) / 65536.0;
    if outline.elements().is_empty() || !(step.is_finite() && step > 0.0) {
        return None;
    }
    let corner = bounds.origin().to_vec2();
    let mut key = Vec::with_capacity(outline.elements().len() * 3 + 1);
    // The grid scales with the outline, so its size, within a 65536th of an
    // octave, tells the same letter at another size apart.
    key.push((step.log2() * 65536.0).round() as i64);
    for element in outline.elements() {
        let (tag, points): (i64, &[kurbo::Point]) = match element {
            kurbo::PathEl::MoveTo(p) => (0, std::slice::from_ref(p)),
            kurbo::PathEl::LineTo(p) => (1, std::slice::from_ref(p)),
            kurbo::PathEl::QuadTo(a, b) => (2, &[*a, *b]),
            kurbo::PathEl::CurveTo(a, b, c) => (3, &[*a, *b, *c]),
            kurbo::PathEl::ClosePath => (4, &[]),
        };
        key.push(tag);
        for point in points {
            let at = *point - corner;
            key.push((at.x / step).round() as i64);
            key.push((at.y / step).round() as i64);
        }
    }
    let shared = match alike.get(&key).and_then(Weak::upgrade) {
        Some(shared) => shared,
        None => {
            let shared = Arc::new(Affine::translate(-corner) * outline);
            alike.insert(key, Arc::downgrade(&shared));
            shared
        }
    };
    Some((shared, corner))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_text(fill: peniko::Color) -> (World, Entity, [Entity; 2]) {
        let mut world = World::new();
        let root = world
            .spawn((
                InWorld3D::default(),
                GroupMarker,
                Visible,
                GlobalSpatialTransform::from_local(&gaanim_math::SpatialTransform::new_2d(
                    1.0, 2.0,
                )),
                GlobalOpacity(1.0),
                RenderLayer::Vello2D,
            ))
            .id();
        let letter = |world: &mut World, x: f64| {
            let local = gaanim_math::SpatialTransform::new_2d(1.0 + x, 2.0);
            world
                .spawn((
                    Visible,
                    GlobalSpatialTransform::from_local(&local),
                    GlobalOpacity(0.5),
                    RenderLayer::Vello2D,
                    // The same letter, carrying its place in its outline.
                    Path2D(Arc::new(
                        kurbo::Rect::new(10.0 + x, 0.0, 10.5 + x, 1.0).to_path(0.0),
                    )),
                    FillBrush::color(fill),
                    ChildOf(root),
                ))
                .id()
        };
        let first = letter(&mut world, 0.0);
        let second = letter(&mut world, 3.0);
        (world, root, [first, second])
    }

    fn run(world: &mut World) {
        let mut schedule = Schedule::default();
        schedule.add_systems(sync_in_world_system);
        schedule.run(world);
    }

    fn image_brush(width: u32, height: u32) -> peniko::ImageBrush {
        peniko::ImageBrush::new(peniko::ImageData {
            data: peniko::Blob::from(vec![255_u8; (width * height * 4) as usize]),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width,
            height,
        })
    }

    /// A Lottie scene becomes the outlines it fills, in its part's units,
    /// with its colors; the rectangles its composition is clipped to cut
    /// none of them.
    #[test]
    fn lottie_scenes_become_outlines() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "gaanim-lottie-in-world-{}-{unique}.json",
            std::process::id()
        ));
        let still = |value: serde_json::Value| serde_json::json!({"a": 0, "k": value});
        let transform = serde_json::json!({
            "a": still(serde_json::json!([0.0, 0.0])),
            "p": still(serde_json::json!([50.0, 50.0])),
            "s": still(serde_json::json!([100.0, 100.0])),
            "r": still(serde_json::json!(0.0)),
            "o": still(serde_json::json!(100.0)),
        });
        let mut group = transform.clone();
        group["ty"] = serde_json::json!("tr");
        group["p"] = still(serde_json::json!([0.0, 0.0]));
        std::fs::write(
            &path,
            serde_json::to_vec(&serde_json::json!({
                "v": "5.7.4", "fr": 30.0, "ip": 0.0, "op": 30.0, "w": 100, "h": 100,
                "assets": [],
                "layers": [{
                    "ddd": 0, "ty": 4, "ind": 1, "st": 0.0, "ip": 0.0, "op": 30.0,
                    "ks": transform,
                    "shapes": [{"ty": "gr", "it": [
                        {"ty": "rc", "s": still(serde_json::json!([40.0, 20.0])),
                         "p": still(serde_json::json!([0.0, 0.0])), "r": still(serde_json::json!(0.0))},
                        {"ty": "fl", "c": still(serde_json::json!([1.0, 0.0, 0.0, 1.0])),
                         "o": still(serde_json::json!(100.0))},
                        group,
                    ]}],
                }],
            }))
            .unwrap(),
        )
        .unwrap();
        let asset = crate::lottie::LottieAsset::load(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        // One unit per pixel of the composition.
        let view = gaanim_objects::prelude::ImageView {
            source_x: 0.0,
            source_y: 0.0,
            source_width: 100.0,
            source_height: 100.0,
            display_width: 100.0,
            display_height: 100.0,
            scale_x: 1.0,
            scale_y: 1.0,
            quality: Default::default(),
        };
        let playback =
            crate::lottie::LottiePlayback::new(asset, view, 0.0, None, false, 1.0).unwrap();
        let mut world = World::new();
        let root = world
            .spawn((
                InWorld3D::default(),
                Visible,
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderLayer::Vello2D,
                Path2D(Arc::new(BezPath::new())),
                LottiePlayer::new(playback),
            ))
            .id();
        run(&mut world);
        let fill = world.get::<PlaneFillData>(root).unwrap();
        assert_eq!(fill.glyphs.len(), 1);
        let glyph = &fill.glyphs[0];
        assert_eq!(glyph.color.to_rgba8().r, 255);
        assert!(glyph.clip.is_none());
        // A rectangle 40 by 20 about the middle, y up.
        let size = glyph.outline.bounding_box().size();
        assert!((size.width - 40.0).abs() < 1e-6 && (size.height - 20.0).abs() < 1e-6);
        let corner = glyph.placement * kurbo::Point::ZERO;
        assert!(
            (corner - kurbo::Point::new(-20.0, -10.0)).hypot() < 1e-6,
            "{corner:?}"
        );
    }

    /// An image part fills its outline with the image, mapped from the
    /// outline's units to its pixels, at the part's opacity.
    #[test]
    fn images_fill_their_outline_in_the_plane() {
        let mut world = World::new();
        let root = world
            .spawn((
                InWorld3D::default(),
                Visible,
                GlobalSpatialTransform::default(),
                GlobalOpacity(1.0),
                RenderLayer::Vello2D,
            ))
            .id();
        let mut brush = image_brush(4, 2);
        brush.sampler.alpha = 0.5;
        world.spawn((
            Visible,
            GlobalSpatialTransform::default(),
            GlobalOpacity(0.5),
            RenderLayer::Vello2D,
            Path2D(Arc::new(kurbo::Rect::new(1.0, 1.0, 3.0, 2.0).to_path(0.0))),
            // A white fill, which the image replaces.
            FillBrush::color(peniko::Color::WHITE),
            RasterImage::new(brush, Affine::translate((1.0, 1.0)) * Affine::scale(0.5)),
            ChildOf(root),
        ));
        run(&mut world);
        let fill = world.get::<PlaneFillData>(root).unwrap();
        assert_eq!(fill.glyphs.len(), 1);
        let glyph = &fill.glyphs[0];
        let image = glyph.image.as_ref().unwrap();
        // The outline lies at its box's corner, (1, 1); its far corner is
        // the image's.
        let far = image.to_pixels * kurbo::Point::new(2.0, 1.0);
        assert!(
            (far - kurbo::Point::new(4.0, 2.0)).hypot() < 1e-9,
            "{far:?}"
        );
        assert_eq!(image.brush.sampler.alpha, 1.0);
        // The part's opacity and the brush's alpha go into the color.
        assert!((glyph.color.components[3] - 0.25).abs() < 1e-6);
    }

    #[test]
    fn billboards_go_to_the_3d_pass_unless_they_hold_what_it_leaves_out() {
        let mut world = World::new();
        // Vector parts carry an empty raster.
        let text = world.spawn((Billboard, RasterImage::none())).id();
        world.spawn((Visible, RasterImage::none(), ChildOf(text)));
        let glowing = world.spawn(Billboard).id();
        world.spawn((Glow::default(), ChildOf(glowing)));
        let chalked = world.spawn(Billboard).id();
        world.spawn((ChalkBrush::default(), ChildOf(chalked)));
        // An image is drawn by the 3D pass too.
        let pictured = world.spawn(Billboard).id();
        world.spawn((
            RasterImage::new(image_brush(2, 2), Affine::IDENTITY),
            ChildOf(pictured),
        ));
        let hud = world.spawn((Billboard, HudOverlay)).id();
        let mut schedule = Schedule::default();
        schedule.add_systems(billboards_in_world_system);
        schedule.run(&mut world);
        assert!(world.get::<InWorld3D>(text).is_some());
        assert!(world.get::<BillboardPlane>(text).is_some());
        assert!(world.get::<InWorld3D>(pictured).is_some());
        // A glowing part glows in the 3D pass; one drawn in chalk keeps it
        // 2D content over the 3D image.
        assert!(world.get::<InWorld3D>(glowing).is_some());
        assert!(world.get::<InWorld3D>(chalked).is_none());
        assert!(world.get::<BillboardOver3D>(chalked).is_some());
        assert!(world.get::<InWorld3D>(hud).is_none());
    }

    #[test]
    fn parts_become_outlines_in_their_drawable_plane() {
        let red = peniko::Color::from_rgb8(255, 0, 0);
        let (mut world, root, parts) = world_with_text(red);
        run(&mut world);
        for part in parts {
            assert!(world.get::<DrawnIn3D>(part).is_some());
        }
        let fill = world.get::<PlaneFillData>(root).unwrap().clone();
        assert_eq!(fill.glyphs.len(), 2);
        // Alike outlines share one, laid at its corner; the placement puts it
        // back where the part draws it, relative to the drawable.
        assert!(Arc::ptr_eq(
            &fill.glyphs[0].outline,
            &fill.glyphs[1].outline
        ));
        let corner = |glyph: &PlaneGlyph| glyph.placement * kurbo::Point::ZERO;
        assert!((corner(&fill.glyphs[0]) - kurbo::Point::new(10.0, 0.0)).hypot() < 1e-9);
        assert!((corner(&fill.glyphs[1]) - kurbo::Point::new(16.0, 0.0)).hypot() < 1e-9);
        assert_eq!(fill.glyphs[0].color, red.multiply_alpha(0.5));
        // Unchanged parts leave the data untouched.
        let tick = world
            .entity(root)
            .get_ref::<PlaneFillData>()
            .unwrap()
            .last_changed();
        world.increment_change_tick();
        run(&mut world);
        let after = world
            .entity(root)
            .get_ref::<PlaneFillData>()
            .unwrap()
            .last_changed();
        assert_eq!(tick, after);
        // A changed color does.
        world.get_mut::<FillBrush>(parts[1]).unwrap().0 =
            Some(peniko::Brush::Solid(peniko::Color::WHITE));
        world.increment_change_tick();
        run(&mut world);
        let fill = world.get::<PlaneFillData>(root).unwrap();
        assert_eq!(
            fill.glyphs[1].color,
            peniko::Color::WHITE.multiply_alpha(0.5)
        );
    }

    #[test]
    fn clip_masks_clip_the_outlines_of_their_part() {
        let (mut world, root, parts) = world_with_text(peniko::Color::BLACK);
        let mask = |path: BezPath, invert| ClipMask {
            path,
            rule: peniko::Fill::NonZero,
            sources: Vec::new(),
            invert,
        };
        // In the part's units: the left half of the first letter.
        let half = kurbo::Rect::new(9.0, -1.0, 10.25, 2.0).to_path(0.0);
        world.entity_mut(parts[0]).insert(mask(half.clone(), false));
        world.entity_mut(parts[1]).insert(mask(half, true));
        run(&mut world);
        let fill = world.get::<PlaneFillData>(root).unwrap();
        let clip = fill.glyphs[0].clip.as_ref().unwrap();
        assert!(!clip.outside);
        // From the letter's corner to the mask's.
        assert_eq!(clip.offset, kurbo::Vec2::new(1.0, 1.0));
        assert!((clip.outline.bounding_box().width() - 1.25).abs() < 1e-9);
        assert!(fill.glyphs[1].clip.as_ref().unwrap().outside);

        // A mask that encloses nothing keeps nothing.
        world
            .entity_mut(parts[0])
            .insert(mask(BezPath::new(), false));
        run(&mut world);
        assert_eq!(world.get::<PlaneFillData>(root).unwrap().glyphs.len(), 1);
    }

    /// A wipe halfway across a strip laid with its x axis along -z keeps
    /// its first half, in the strip's own units; before the wipe's cursor,
    /// nothing.
    #[test]
    fn wipes_sweep_in_world_drawables_in_their_own_plane() {
        use gaanim_animation::{EndpointFollow, FollowOffsetSpace, TrackingEndpoint};
        use gaanim_core::glam::DVec3;

        let mut world = World::new();
        world.insert_resource(gaanim_animation::PlaybackState::default());
        let mut laid = gaanim_math::SpatialTransform::default();
        laid.rotation =
            gaanim_math::orientation_from_axes(DVec3::NEG_Z, DVec3::NEG_X).expect("valid axes");
        let root = world
            .spawn((
                InWorld3D::default(),
                GlobalSpatialTransform::from_local(&laid),
            ))
            .id();
        let anchor = |x: f64| TrackingEndpoint::EntityAnchor {
            entity: root,
            normalized: DVec3::new(x, 0.0, 0.0),
            offset: DVec3::ZERO,
        };
        let mask = world
            .spawn((
                Path2D(Arc::new(
                    kurbo::Rect::new(-1000.0, -1000.0, 1000.0, 1000.0).to_path(0.1),
                )),
                gaanim_math::SpatialTransform::default(),
            ))
            .id();
        let curve = world
            .spawn((
                gaanim_scene::LocalBounds(gaanim_math::Bounds3D::new_2d(0.0, -1.0, 10.0, 1.0)),
                GlobalSpatialTransform::from_local(&laid),
                ClipMask {
                    path: BezPath::new(),
                    rule: peniko::Fill::NonZero,
                    sources: vec![mask],
                    invert: false,
                },
                ChildOf(root),
            ))
            .id();
        world
            .entity_mut(root)
            .insert(gaanim_scene::LocalBounds(gaanim_math::Bounds3D::new_2d(
                0.0, -1.0, 10.0, 1.0,
            )));
        let follow = |since: f64| EndpointFollow {
            endpoint: TrackingEndpoint::Between {
                from: Box::new(anchor(-1.0)),
                to: Box::new(anchor(1.0)),
                alpha: 0.5.into(),
                offset: DVec3::new(-1000.0, 0.0, 0.0),
            },
            offset: DVec3::ZERO,
            offset_space: FollowOffsetSpace::World,
            since,
        };
        world.entity_mut(mask).insert(follow(0.0));
        resolve_in_world_wipes_system(&mut world);
        let kept = world.get::<ClipMask>(curve).unwrap().path.bounding_box();
        assert!((kept.x1 - 5.0).abs() < 1e-9, "{kept:?}");
        assert!(
            kept.x0 < -1000.0 && kept.y0 < -100.0 && kept.y1 > 100.0,
            "{kept:?}"
        );
        world.entity_mut(mask).insert(follow(1.0));
        resolve_in_world_wipes_system(&mut world);
        assert!(
            world
                .get::<ClipMask>(curve)
                .unwrap()
                .path
                .elements()
                .is_empty()
        );
    }

    #[test]
    fn inverted_masks_keep_their_own_contour() {
        let contour = kurbo::Circle::new((0.0, 0.0), 1.0).to_path(0.1);
        let mut enclosed = kurbo::Rect::new(-9.0, -9.0, 9.0, 9.0).to_path(0.1);
        enclosed.extend(contour.iter());
        let mask = ClipMask {
            path: enclosed,
            rule: peniko::Fill::EvenOdd,
            sources: vec![Entity::PLACEHOLDER],
            invert: true,
        };
        assert_eq!(mask.kept_contour(), (contour, true));
    }

    #[test]
    fn a_stroke_becomes_an_outline_of_its_own() {
        let mut stroked = StrokeBrush::new(peniko::Color::BLACK, 0.2);
        stroked.brush = Some(peniko::Brush::Solid(peniko::Color::BLACK));
        let recipe = FragmentRecipe {
            path: Some(Arc::new(
                kurbo::Line::new((0.0, 0.0), (2.0, 0.0)).to_path(0.0),
            )),
            stroke: Some(stroked),
            ..Default::default()
        };
        let drawn = drawn(&recipe, &mut HashMap::new());
        assert_eq!(drawn.len(), 1);
        let Laid {
            outline,
            corner,
            paint,
            clip,
        } = &drawn[0];
        assert_eq!(*paint, Paint::Solid(peniko::Color::BLACK));
        // An open line strokes its centerline.
        assert!(clip.is_none());
        let bounds = outline.bounding_box() + *corner;
        assert!((bounds.height() - 0.2).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.width() - 2.0).abs() < 0.21, "{bounds:?}");
    }

    #[test]
    fn strokes_aligned_inside_or_outside_are_twice_as_wide_and_clipped() {
        let mut stroked = StrokeBrush::new(peniko::Color::BLACK, 0.2);
        stroked.brush = Some(peniko::Brush::Solid(peniko::Color::BLACK));
        let square = kurbo::Rect::new(1.0, 1.0, 3.0, 3.0).to_path(0.0);
        let recipe = |align| FragmentRecipe {
            path: Some(Arc::new(square.clone())),
            stroke: Some(stroked.clone()),
            stroke_align: align,
            ..Default::default()
        };
        for (align, outside) in [(StrokeAlign::Inside, false), (StrokeAlign::Outside, true)] {
            let drawn = drawn(&recipe(align), &mut HashMap::new());
            let laid = &drawn[0];
            let (contour, corner, kept_outside) = laid.clip.clone().unwrap();
            assert_eq!(kept_outside, outside);
            // The contour is the square, laid at its corner.
            assert_eq!(corner, kurbo::Vec2::new(1.0, 1.0));
            assert!((contour.bounding_box().width() - 2.0).abs() < 1e-9);
            // The stroke reaches its whole width beyond the contour.
            let bounds = laid.outline.bounding_box() + laid.corner;
            assert!((bounds.x0 - 0.8).abs() < 1e-6, "{bounds:?}");
        }
        let centered = drawn(&recipe(StrokeAlign::Center), &mut HashMap::new());
        assert!(centered[0].clip.is_none());
        let bounds = centered[0].outline.bounding_box() + centered[0].corner;
        assert!((bounds.x0 - 0.9).abs() < 1e-6, "{bounds:?}");
    }

    #[test]
    fn a_gradient_fills_its_outlines_in_the_part_units() {
        let (red, blue) = (
            peniko::Color::from_rgb8(255, 0, 0),
            peniko::Color::from_rgb8(0, 0, 255),
        );
        let (mut world, root, parts) = world_with_text(red);
        let gradient =
            peniko::Gradient::new_linear((10.0, 0.0), (14.0, 0.0)).with_stops([red, blue]);
        world.get_mut::<FillBrush>(parts[1]).unwrap().0 =
            Some(peniko::Brush::Gradient(gradient.clone()));
        run(&mut world);
        let fill = world.get::<PlaneFillData>(root).unwrap();
        assert!(fill.glyphs[0].gradient.is_none());
        let glyph = &fill.glyphs[1];
        let (painted, to_gradient) = glyph.gradient.as_ref().unwrap();
        // At the part's opacity, its first color drawn by older readers.
        assert_eq!(**painted, gradient.multiply_alpha(0.5));
        assert_eq!(glyph.color, red.multiply_alpha(0.5));
        // The outline, laid at its corner, maps back to the part's units.
        let start = *to_gradient * kurbo::Point::ZERO;
        assert!(
            (start - kurbo::Point::new(13.0, 0.0)).hypot() < 1e-9,
            "{start:?}"
        );
    }
}
