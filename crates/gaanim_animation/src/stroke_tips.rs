//! Arrowheads and dots on the ends of any stroked path.
//!
//! A [`StrokeTips`] path keeps its authored geometry. Right before the
//! renderer extracts it, the path is shortened under its arrowheads and the
//! tip entities (children of the path) receive their shapes, taken from the
//! path's current ends; right after, the authored path is restored. Tips
//! therefore follow trims, `create`, connectors and any regenerated path, and
//! timeline animations, snapshots and seeks never see the shortened path.

use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::kurbo::{self, BezPath, PathEl, Point, Shape, Vec2};
use gaanim_scene::{FillBrush, Path2D, RenderOrder, StrokeBrush, Visible};

/// Shape drawn on one end of a stroke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipKind {
    /// Filled triangle whose apex is the end of the path.
    Arrow,
    /// Filled disk centered on the end of the path.
    Dot,
}

/// Marker of the child entity that draws one tip.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct StrokeTip;

#[derive(Debug, Clone)]
struct TipCache {
    source: Arc<BezPath>,
    stroke_width: f64,
    shortened: Arc<BezPath>,
    start: Arc<BezPath>,
    end: Arc<BezPath>,
}

/// Component: tips on the start and end of the entity's path.
#[derive(Component, Debug, Clone)]
pub struct StrokeTips {
    pub start: Option<TipKind>,
    pub end: Option<TipKind>,
    /// Arrowhead length in scene units; `None` scales with the stroke width.
    pub length: Option<f64>,
    /// Arrowhead base width or dot diameter; `None` scales with the length.
    pub width: Option<f64>,
    pub start_entity: Option<Entity>,
    pub end_entity: Option<Entity>,
    applied: Option<Arc<BezPath>>,
    cache: Option<TipCache>,
}

impl StrokeTips {
    pub fn new(
        start: Option<TipKind>,
        end: Option<TipKind>,
        length: Option<f64>,
        width: Option<f64>,
    ) -> Self {
        Self {
            start,
            end,
            length,
            width,
            start_entity: None,
            end_entity: None,
            applied: None,
            cache: None,
        }
    }

    /// Arrowhead length and width for a stroke of `stroke_width`.
    pub fn arrow_size(&self, stroke_width: f64) -> (f64, f64) {
        let length = self.length.unwrap_or((5.0 * stroke_width).max(0.15));
        (length, self.width.unwrap_or(0.9 * length))
    }

    /// Dot diameter for a stroke of `stroke_width`.
    pub fn dot_size(&self, stroke_width: f64) -> f64 {
        self.width.unwrap_or((3.0 * stroke_width).max(0.12))
    }

    /// The path shortened under its arrowheads and the start and end shapes.
    ///
    /// Arrowheads shrink with their proportions intact when the path is
    /// shorter than they are, so a path drawn from nothing grows its head.
    pub fn geometry(&self, path: &BezPath, stroke_width: f64) -> (BezPath, BezPath, BezPath) {
        let total = path.perimeter(1e-4);
        let (Some(first), Some(last)) = (first_point(path), last_point(path)) else {
            return (path.clone(), BezPath::new(), BezPath::new());
        };
        if !(total > 1e-9) {
            return (path.clone(), BezPath::new(), BezPath::new());
        }
        let (length, width) = self.arrow_size(stroke_width);
        let arrows = [self.start, self.end]
            .iter()
            .filter(|kind| **kind == Some(TipKind::Arrow))
            .count();
        let scale = if arrows == 0 {
            1.0
        } else {
            (total / (length * arrows as f64)).min(1.0)
        };
        let cut = |kind| {
            if kind == Some(TipKind::Arrow) {
                // A lone head on a short path keeps a sliver of it as its base.
                (length * scale / total).min(1.0 - 1e-6)
            } else {
                0.0
            }
        };
        let (start_cut, end_cut) = (cut(self.start), cut(self.end));
        let shortened = if start_cut > 0.0 || end_cut > 0.0 {
            gaanim_math::trim_path(path, start_cut, 1.0 - end_cut, 0.0, true)
        } else {
            path.clone()
        };
        let shape = |kind, apex: Point, base: Option<Point>| match kind {
            Some(TipKind::Arrow) => base
                .map(|base| arrowhead(apex, base, width * scale))
                .unwrap_or_default(),
            Some(TipKind::Dot) => kurbo::Circle::new(apex, self.dot_size(stroke_width) / 2.0)
                .path_elements(1e-3)
                .collect(),
            None => BezPath::new(),
        };
        // Each base is measured on its own: two heads may cover the whole path.
        let start_base = first_point(&gaanim_math::trim_path(path, start_cut, 1.0, 0.0, true));
        let end_base = last_point(&gaanim_math::trim_path(path, 0.0, 1.0 - end_cut, 0.0, true));
        let start = shape(self.start, first, start_base);
        let end = shape(self.end, last, end_base);
        (shortened, start, end)
    }
}

fn first_point(path: &BezPath) -> Option<Point> {
    match path.elements().first()? {
        PathEl::MoveTo(point) => Some(*point),
        _ => None,
    }
}

/// Where the stroke ends: a closing segment returns to its sub-path's start.
fn last_point(path: &BezPath) -> Option<Point> {
    let (mut start, mut current, mut drawn) = (None, None, false);
    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => {
                start = Some(*point);
                current = Some(*point);
            }
            PathEl::LineTo(point) | PathEl::QuadTo(_, point) | PathEl::CurveTo(_, _, point) => {
                current = Some(*point);
                drawn = true;
            }
            PathEl::ClosePath => current = start,
        }
    }
    current.filter(|_| drawn)
}

/// Triangle with its apex at `apex` and its base centered on `base`.
fn arrowhead(apex: Point, base: Point, width: f64) -> BezPath {
    let axis: Vec2 = apex - base;
    let length = axis.hypot();
    let mut path = BezPath::new();
    if length < 1e-9 || width <= 0.0 {
        return path;
    }
    let normal = Vec2::new(-axis.y, axis.x) * (width / 2.0 / length);
    path.move_to(base + normal);
    path.line_to(apex);
    path.line_to(base - normal);
    path.close_path();
    path
}

type TipQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Path2D,
        &'static mut FillBrush,
        &'static mut RenderOrder,
        Has<Visible>,
    ),
    (With<StrokeTip>, Without<StrokeTips>),
>;

/// Shortens tipped paths and shapes their tips right before extraction.
pub fn apply_stroke_tips_system(
    mut commands: Commands,
    mut paths: Query<
        (
            &mut StrokeTips,
            &mut Path2D,
            &StrokeBrush,
            Option<&FillBrush>,
            &RenderOrder,
            Has<Visible>,
        ),
        Without<StrokeTip>,
    >,
    mut tips: TipQuery,
) {
    for (mut stroke_tips, mut path, stroke, fill, order, visible) in &mut paths {
        let source = path.0.clone();
        let width = stroke.style.width;
        let cache = match &stroke_tips.cache {
            Some(cache) if Arc::ptr_eq(&cache.source, &source) && cache.stroke_width == width => {
                cache.clone()
            }
            _ => {
                let (shortened, start, end) = stroke_tips.geometry(&source, width);
                let cache = TipCache {
                    source: source.clone(),
                    stroke_width: width,
                    shortened: Arc::new(shortened),
                    start: Arc::new(start),
                    end: Arc::new(end),
                };
                stroke_tips.cache = Some(cache.clone());
                cache
            }
        };
        // Shortening a filled shape would cut its fill: its heads sit on top.
        let filled = fill.is_some_and(|fill| fill.0.is_some());
        if !filled && !Arc::ptr_eq(&cache.shortened, &source) && *cache.shortened != *source {
            path.0 = cache.shortened.clone();
            stroke_tips.applied = Some(source);
        }
        for (entity, shape) in [
            (stroke_tips.start_entity, &cache.start),
            (stroke_tips.end_entity, &cache.end),
        ] {
            let Some(entity) = entity else {
                continue;
            };
            let Ok((mut tip_path, mut fill, mut tip_order, tip_visible)) = tips.get_mut(entity)
            else {
                continue;
            };
            if !Arc::ptr_eq(&tip_path.0, shape) {
                tip_path.0 = shape.clone();
            }
            if fill.0 != stroke.brush {
                fill.0 = stroke.brush.clone();
            }
            if *tip_order != *order {
                *tip_order = *order;
            }
            // The tip shows exactly when its path does.
            if visible && !tip_visible {
                commands.entity(entity).insert(Visible);
            } else if !visible && tip_visible {
                commands.entity(entity).remove::<Visible>();
            }
        }
    }
}

/// Restores the authored paths after extraction.
pub fn restore_stroke_tips_system(mut paths: Query<(&mut StrokeTips, &mut Path2D)>) {
    for (mut stroke_tips, mut path) in &mut paths {
        if let Some(source) = stroke_tips.applied.take() {
            path.0 = source;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(length: f64) -> BezPath {
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((length, 0.0));
        path
    }

    #[test]
    fn arrowheads_sit_on_the_ends_and_shorten_the_stroke() {
        let tips = StrokeTips::new(
            Some(TipKind::Dot),
            Some(TipKind::Arrow),
            Some(0.4),
            Some(0.3),
        );
        let (shortened, start, end) = tips.geometry(&line(2.0), 0.05);
        assert!((last_point(&shortened).unwrap().x - 1.6).abs() < 1e-6);
        assert_eq!(first_point(&shortened), Some(Point::ORIGIN));
        let head = end.bounding_box();
        assert!((head.x0 - 1.6).abs() < 1e-6 && (head.x1 - 2.0).abs() < 1e-6);
        assert!((head.height() - 0.3).abs() < 1e-6);
        let dot = start.bounding_box();
        assert!((dot.width() - 0.3).abs() < 1e-3 && dot.center().distance(Point::ORIGIN) < 1e-6);
    }

    #[test]
    fn short_paths_shrink_their_heads() {
        let tips = StrokeTips::new(
            Some(TipKind::Arrow),
            Some(TipKind::Arrow),
            Some(0.4),
            Some(0.4),
        );
        let (shortened, start, end) = tips.geometry(&line(0.4), 0.05);
        // Each head takes half of the path, at half its size.
        assert!(shortened.perimeter(1e-6) < 1e-6);
        assert!((end.bounding_box().height() - 0.2).abs() < 1e-6);
        assert!((start.bounding_box().width() - 0.2).abs() < 1e-6);
        let lone = StrokeTips::new(None, Some(TipKind::Arrow), Some(0.4), Some(0.4));
        let (_, _, end) = lone.geometry(&line(0.2), 0.05);
        assert!((end.bounding_box().width() - 0.2).abs() < 1e-5);
        let (_, _, empty) = tips.geometry(&BezPath::new(), 0.05);
        assert!(empty.elements().is_empty());
    }

    #[test]
    fn tips_touch_only_the_extracted_path() {
        let mut world = World::new();
        let tip = world
            .spawn((
                StrokeTip,
                Path2D(Arc::new(BezPath::new())),
                FillBrush(None),
                RenderOrder::default(),
            ))
            .id();
        let mut tips = StrokeTips::new(None, Some(TipKind::Arrow), Some(0.5), None);
        tips.end_entity = Some(tip);
        let authored = Arc::new(line(3.0));
        let order = RenderOrder {
            z_index: 2,
            ..Default::default()
        };
        let route = world
            .spawn((
                tips,
                Path2D(authored.clone()),
                StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.05),
                order,
                Visible,
            ))
            .id();
        let mut apply = IntoSystem::into_system(apply_stroke_tips_system);
        apply.initialize(&mut world);
        apply.run((), &mut world).unwrap();
        apply.apply_deferred(&mut world);
        let shortened = world.get::<Path2D>(route).unwrap().0.clone();
        assert!((last_point(&shortened).unwrap().x - 2.5).abs() < 1e-6);
        assert!(!world.get::<Path2D>(tip).unwrap().0.elements().is_empty());
        assert!(world.get::<FillBrush>(tip).unwrap().0.is_some());
        assert_eq!(*world.get::<RenderOrder>(tip).unwrap(), order);
        assert!(world.get::<Visible>(tip).is_some());

        let mut restore = IntoSystem::into_system(restore_stroke_tips_system);
        restore.initialize(&mut world);
        restore.run((), &mut world).unwrap();
        assert!(Arc::ptr_eq(
            &world.get::<Path2D>(route).unwrap().0,
            &authored
        ));
    }

    #[test]
    fn odd_paths_place_tips_on_their_drawn_ends() {
        let tips = StrokeTips::new(
            Some(TipKind::Dot),
            Some(TipKind::Arrow),
            Some(0.2),
            Some(0.2),
        );
        // A closed triangle ends where it started.
        let mut triangle = BezPath::new();
        triangle.move_to((0.0, 0.0));
        triangle.line_to((2.0, 0.0));
        triangle.line_to((2.0, 2.0));
        triangle.close_path();
        assert_eq!(last_point(&triangle), Some(Point::ORIGIN));
        let (_, _, head) = tips.geometry(&triangle, 0.05);
        let apex_distance = head
            .elements()
            .iter()
            .filter_map(|element| match element {
                PathEl::LineTo(point) => Some(point.distance(Point::ORIGIN)),
                _ => None,
            })
            .fold(f64::INFINITY, f64::min);
        assert!(apex_distance < 1e-6, "{apex_distance}");

        // Separate sub-paths: tips go on the first start and the last end.
        let mut two = line(1.0);
        two.move_to((0.0, 3.0));
        two.line_to((1.0, 3.0));
        let (_, start, end) = tips.geometry(&two, 0.05);
        assert!(start.bounding_box().center().distance(Point::ORIGIN) < 1e-6);
        assert!((end.bounding_box().x1 - 1.0).abs() < 1e-6);
        assert!((end.bounding_box().center().y - 3.0).abs() < 1e-6);

        // Degenerate paths draw no tips and keep their geometry.
        let mut dot = BezPath::new();
        dot.move_to((1.0, 1.0));
        dot.line_to((1.0, 1.0));
        let mut lone_move = BezPath::new();
        lone_move.move_to((1.0, 1.0));
        for path in [dot, lone_move, BezPath::new()] {
            let (kept, start, end) = tips.geometry(&path, 0.05);
            assert_eq!(kept, path);
            assert!(start.elements().is_empty() && end.elements().is_empty());
        }
    }

    #[test]
    fn filled_shapes_keep_their_outline_under_tips() {
        let mut world = World::new();
        let tip = world
            .spawn((
                StrokeTip,
                Path2D(Arc::new(BezPath::new())),
                FillBrush(None),
                RenderOrder::default(),
            ))
            .id();
        let mut tips = StrokeTips::new(None, Some(TipKind::Arrow), Some(0.5), None);
        tips.end_entity = Some(tip);
        let authored = Arc::new(line(3.0));
        let route = world
            .spawn((
                tips,
                Path2D(authored.clone()),
                StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.05),
                FillBrush::color(gaanim_core::peniko::Color::BLACK),
                RenderOrder::default(),
                Visible,
            ))
            .id();
        let mut apply = IntoSystem::into_system(apply_stroke_tips_system);
        apply.initialize(&mut world);
        apply.run((), &mut world).unwrap();
        assert!(Arc::ptr_eq(
            &world.get::<Path2D>(route).unwrap().0,
            &authored
        ));
        assert!(!world.get::<Path2D>(tip).unwrap().0.elements().is_empty());
    }
}
