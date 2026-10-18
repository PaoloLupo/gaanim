//! 2D drawables placed in the 3D world ([`InWorld3D`]). Each frame the
//! outlines their parts draw, filled and stroked as the 2D renderer would
//! draw them, become the drawable's [`PlaneFillData`], which the 3D pass
//! fills from their curves (see [`crate::plane_fill`]); the 2D renderer
//! leaves the parts, marked [`DrawnIn3D`], to it.
//!
//! A part's fill and stroke keep their colors, its opacity and the progress
//! of a write; gradients draw in their first color, and shadows, glows,
//! blurs, chalk, rasters and Lottie scenes are left out.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Weak};

use bevy::prelude::*;
use gaanim_animation::{FillDrawProgress, WriteTipGlow};
use gaanim_core::kurbo::{self, Affine, BezPath, Shape};
use gaanim_core::peniko;
use gaanim_math::GlobalSpatialTransform;
use gaanim_scene::{
    DrawnIn3D, FillBrush, GlobalOpacity, GroupMarker, InWorld3D, Path2D, PathSource, PlaneFillData,
    PlaneGlyph, RenderLayer, StrokeBrush, Visible,
};

use crate::effects::{StrokeAlign, StrokeProfile, StrokeScalesWithObject};
use crate::fragment::{FragmentParts, FragmentRecipe, fragment_recipe};

/// What a part draws at full opacity: outlines laid at their box's corner,
/// that corner, and their colors.
type Drawn = Arc<[(Arc<BezPath>, kurbo::Vec2, peniko::Color)]>;

/// The outlines parts drew, by part, while their recipe stays the same, and
/// outlines alike, shared: the same letter again packs and records once.
#[derive(Default)]
pub(crate) struct InWorldParts {
    parts: HashMap<Entity, (FragmentRecipe, Drawn)>,
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
    Has<StrokeScalesWithObject>,
);

/// Keep the [`PlaneFillData`] of each [`InWorld3D`] drawable: the outlines
/// its visible parts draw, placed in its plane relative to it, at their
/// opacity within it. A drawable whose outlines did not change keeps its
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
        ),
        With<InWorld3D>,
    >,
    children: Query<&Children>,
    parts: Query<PartData>,
    mut held: Local<InWorldParts>,
) {
    let held = &mut *held;
    let mut seen = HashSet::new();
    for (root, root_transform, root_opacity, current) in &roots {
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
                stroke_scales,
            )) = parts.get(entity)
            else {
                continue;
            };
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
                raster: None,
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
            let placement = to_root * transform.affine_2d;
            let alpha = (opacity.0 / root_opacity).clamp(0.0, 1.0);
            for (outline, corner, color) in drawn.iter() {
                glyphs.push(PlaneGlyph {
                    outline: Arc::clone(outline),
                    placement: placement * Affine::translate(*corner),
                    color: color.multiply_alpha(alpha),
                });
            }
        }
        if current.is_none_or(|current| !same_glyphs(&current.glyphs, &glyphs)) {
            commands.entity(root).insert(PlaneFillData { glyphs });
        }
    }
    held.parts.retain(|entity, _| seen.contains(entity));
    if held.alike.len() >= 64 && held.alike.len() >= 2 * held.swept {
        held.alike.retain(|_, outline| outline.strong_count() > 0);
        held.swept = held.alike.len();
    }
}

/// Whether two lists of glyphs draw the same: outlines compared by address
/// first, as unchanged parts keep theirs.
fn same_glyphs(a: &[PlaneGlyph], b: &[PlaneGlyph]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.placement == b.placement
                && a.color == b.color
                && (Arc::ptr_eq(&a.outline, &b.outline) || a.outline == b.outline)
        })
}

/// The outlines `recipe` fills and strokes, as the 2D renderer draws them
/// (see [`crate::fragment::build_fragment`]): the fill unless a write is
/// still drawing a closed outline, faded by the write's fill progress, and
/// the stroke widened into an outline of its own.
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
    if !trimmed_closed
        && fill_alpha > 0.0
        && let Some(color) = recipe.fill.as_ref().and_then(brush_color)
    {
        drawn.extend(laid(path.clone(), color.multiply_alpha(fill_alpha), alike));
    }
    if let Some(stroke) = &recipe.stroke
        && let Some(brush) = &stroke.brush
    {
        let (brush, style) = crate::pipeline::animated_stroke_paint(brush, &stroke.style, wave);
        let style = crate::stroke::drawable_pen(&style);
        if let Some(color) = brush_color(&brush)
            && style.width.is_finite()
            && style.width > 0.0
        {
            let widen = |path: &BezPath| {
                let tolerance = (style.width * 0.01).max(1e-9);
                kurbo::stroke(
                    path.iter(),
                    &style,
                    &kurbo::StrokeOpts::default(),
                    tolerance,
                )
            };
            let outline = match recipe.stroke_view {
                Some(view) => view.inverse() * widen(&(view * path.clone())),
                None => widen(path),
            };
            drawn.extend(laid(outline, color, alike));
        }
    }
    drawn.into()
}

/// The color a brush paints with: a gradient's first, none for an image.
fn brush_color(brush: &peniko::Brush) -> Option<peniko::Color> {
    match brush {
        peniko::Brush::Solid(color) => Some(*color),
        peniko::Brush::Gradient(gradient) => gradient
            .stops
            .first()
            .map(|stop| stop.color.to_alpha_color()),
        peniko::Brush::Image(_) => None,
    }
}

/// `outline` laid at its box's corner, shared with an outline alike, with
/// that corner and `color`; nothing for an empty outline.
fn laid(
    outline: BezPath,
    color: peniko::Color,
    alike: &mut HashMap<Vec<i64>, Weak<BezPath>>,
) -> Option<(Arc<BezPath>, kurbo::Vec2, peniko::Color)> {
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
    Some((shared, corner, color))
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
        let (outline, corner, color) = &drawn[0];
        assert_eq!(*color, peniko::Color::BLACK);
        let bounds = outline.bounding_box() + *corner;
        assert!((bounds.height() - 0.2).abs() < 1e-6, "{bounds:?}");
        assert!((bounds.width() - 2.0).abs() < 0.21, "{bounds:?}");
    }
}
