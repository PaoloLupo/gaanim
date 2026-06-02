use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{self, Shape};
use gaanim_math::{Bounds3D, GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, LocalBounds, MobjectId, ObjectTag, Opacity, Path2D, PathSource,
    RenderLayer, RenderOrder, StrokeBrush, Visible,
};

/// A standard, complete Bevy Bundle representing a 2D vector Mobject.
///
/// This groups all essential visual, spatial, and layering components together,
/// making it extremely easy to spawn high-level objects using standard Bevy commands.
#[derive(Bundle, Clone)]
pub struct MobjectBundle {
    pub id: MobjectId,
    pub path: Path2D,
    pub path_source: PathSource,
    pub bounds: LocalBounds,
    pub transform: SpatialTransform,
    pub global_transform: GlobalSpatialTransform,
    pub fill: FillBrush,
    pub stroke: StrokeBrush,
    pub opacity: Opacity,
    pub global_opacity: GlobalOpacity,
    pub render_order: RenderOrder,
    pub render_layer: RenderLayer,
    pub visible: Visible,
    pub tag: ObjectTag,
}

impl MobjectBundle {
    /// Creates a new `MobjectBundle` with custom geometry and boundaries.
    ///
    /// By default, it initializes as a white-filled shape with no outline, located
    /// at the viewport origin `(0, 0)`, fully opaque and visible on the Vello2D rendering layer.
    pub fn new(id: ObjectId, path: kurbo::BezPath, bounds: Bounds3D) -> Self {
        let arc_path = std::sync::Arc::new(path);
        Self {
            id: MobjectId(id),
            path: Path2D(arc_path.clone()),
            path_source: PathSource(arc_path),
            bounds: LocalBounds(bounds),
            transform: SpatialTransform::identity(),
            global_transform: GlobalSpatialTransform::default(),
            fill: FillBrush(Some(gaanim_core::peniko::Brush::Solid(
                gaanim_core::peniko::Color::WHITE,
            ))),
            stroke: StrokeBrush::transparent(),
            opacity: Opacity(1.0),
            global_opacity: GlobalOpacity(1.0),
            render_order: RenderOrder {
                z_index: 0,
                creation_order: id.index() as u64,
            },
            render_layer: RenderLayer::Vello2D,
            visible: Visible,
            tag: ObjectTag("Mobject".into()),
        }
    }
}

/// Creates a circle Mobject bundle.
pub fn circle(id: ObjectId, radius: f64) -> MobjectBundle {
    let path = kurbo::Circle::new(kurbo::Point::ZERO, radius).to_path(0.1);
    let bounds = Bounds3D::new_2d(-radius, -radius, radius, radius);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Circle".into());
    bundle
}

/// Creates a rectangle Mobject bundle.
pub fn rectangle(id: ObjectId, width: f64, height: f64) -> MobjectBundle {
    let w2 = width / 2.0;
    let h2 = height / 2.0;
    let path = kurbo::Rect::new(-w2, -h2, w2, h2).to_path(0.1);
    let bounds = Bounds3D::new_2d(-w2, -h2, w2, h2);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Rectangle".into());
    bundle
}

/// Creates a rounded rectangle Mobject bundle.
pub fn rounded_rect(id: ObjectId, width: f64, height: f64, corner_radius: f64) -> MobjectBundle {
    let w2 = width / 2.0;
    let h2 = height / 2.0;
    let path = kurbo::RoundedRect::new(-w2, -h2, w2, h2, corner_radius).to_path(0.1);
    let bounds = Bounds3D::new_2d(-w2, -h2, w2, h2);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("RoundedRectangle".into());
    bundle
}

/// Creates a line segment Mobject bundle.
pub fn line(id: ObjectId, start: kurbo::Point, end: kurbo::Point) -> MobjectBundle {
    let path = kurbo::Line::new(start, end).to_path(0.1);
    let min_x = start.x.min(end.x);
    let max_x = start.x.max(end.x);
    let min_y = start.y.min(end.y);
    let max_y = start.y.max(end.y);
    let bounds = Bounds3D::new_2d(min_x, min_y, max_x, max_y);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Line".into());
    bundle
}

/// Creates a circular or elliptical arc segment Mobject bundle.
pub fn arc(
    id: ObjectId,
    center: kurbo::Point,
    radii: kurbo::Vec2,
    start_angle: f64,
    sweep_angle: f64,
    x_rotation: f64,
) -> MobjectBundle {
    let arc_geom = kurbo::Arc::new(center, radii, start_angle, sweep_angle, x_rotation);
    let path = arc_geom.to_path(0.1);
    let bounding_rect = arc_geom.bounding_box();
    let bounds = Bounds3D::new_2d(
        bounding_rect.x0,
        bounding_rect.y0,
        bounding_rect.x1,
        bounding_rect.y1,
    );
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Arc".into());
    bundle
}

/// Creates a general closed polygon Mobject bundle from a list of vertices.
pub fn polygon(id: ObjectId, points: &[kurbo::Point]) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    if !points.is_empty() {
        path.move_to(points[0]);
        for &p in &points[1..] {
            path.line_to(p);
        }
        path.close_path();
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for &p in points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    let bounds = if points.is_empty() {
        Bounds3D::default()
    } else {
        Bounds3D::new_2d(min_x, min_y, max_x, max_y)
    };
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Polygon".into());
    bundle
}

/// Creates a symmetric star Mobject bundle.
pub fn star(id: ObjectId, n_points: u32, outer_radius: f64, inner_radius: f64) -> MobjectBundle {
    use std::f64::consts::PI;
    let mut points = Vec::new();
    let steps = 2 * n_points;
    for i in 0..steps {
        let angle = i as f64 * PI / n_points as f64 - PI / 2.0;
        let r = if i % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let x = r * angle.cos();
        let y = r * angle.sin();
        points.push(kurbo::Point::new(x, y));
    }
    let mut bundle = polygon(id, &points);
    bundle.tag = ObjectTag("Star".into());
    bundle
}

/// Creates an ellipse Mobject bundle.
pub fn ellipse(id: ObjectId, rx: f64, ry: f64) -> MobjectBundle {
    let ell_geom = kurbo::Ellipse::new(kurbo::Point::ZERO, (rx, ry), 0.0);
    let path = ell_geom.to_path(0.1);
    let bounds = Bounds3D::new_2d(-rx, -ry, rx, ry);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Ellipse".into());
    bundle
}

/// Creates a tiny dot (represented as a circle) Mobject bundle.
pub fn dot(id: ObjectId, radius: f64) -> MobjectBundle {
    let mut bundle = circle(id, radius);
    bundle.tag = ObjectTag("Dot".into());
    bundle
}

/// Creates a square Mobject bundle.
pub fn square(id: ObjectId, side_length: f64) -> MobjectBundle {
    let mut bundle = rectangle(id, side_length, side_length);
    bundle.tag = ObjectTag("Square".into());
    bundle
}

/// Creates a triangle Mobject bundle from three vertices.
pub fn triangle(
    id: ObjectId,
    p1: kurbo::Point,
    p2: kurbo::Point,
    p3: kurbo::Point,
) -> MobjectBundle {
    let mut bundle = polygon(id, &[p1, p2, p3]);
    bundle.tag = ObjectTag("Triangle".into());
    bundle
}

/// Creates a regular polygon Mobject bundle with N sides.
pub fn regular_polygon(id: ObjectId, n_sides: u32, radius: f64) -> MobjectBundle {
    use std::f64::consts::PI;
    let mut points = Vec::new();
    for i in 0..n_sides {
        let angle = i as f64 * 2.0 * PI / n_sides as f64 - PI / 2.0;
        let x = radius * angle.cos();
        let y = radius * angle.sin();
        points.push(kurbo::Point::new(x, y));
    }
    let mut bundle = polygon(id, &points);
    bundle.tag = ObjectTag(format!("RegularPolygon({})", n_sides));
    bundle
}

/// Creates a checkmark ✓ Mobject bundle (drawn stroke-only by default).
pub fn checkmark(id: ObjectId, size: f64) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    path.move_to(kurbo::Point::new(-0.4 * size, -0.1 * size));
    path.line_to(kurbo::Point::new(-0.15 * size, -0.35 * size));
    path.line_to(kurbo::Point::new(0.4 * size, 0.4 * size));
    let bounds = Bounds3D::new_2d(-0.4 * size, -0.35 * size, 0.4 * size, 0.4 * size);
    let mut bundle = MobjectBundle::new(id, path, bounds);

    // Checkmarks default to stroke-only
    bundle.fill = FillBrush(None);
    bundle.stroke = StrokeBrush {
        brush: Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        )),
        style: kurbo::Stroke::new(3.0),
    };
    bundle.tag = ObjectTag("Checkmark".into());
    bundle
}

/// Creates a directional arrow Mobject bundle with a solid triangular head.
pub fn arrow(id: ObjectId, start: kurbo::Point, end: kurbo::Point) -> MobjectBundle {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();
    if len > 0.0 {
        let ux = dx / len;
        let uy = dy / len;

        let head_len = 15.0;
        let head_half_width = 7.5;

        let base_x = end.x - ux * head_len;
        let base_y = end.y - uy * head_len;

        path.move_to(start);
        path.line_to(kurbo::Point::new(base_x, base_y));

        let perp_x = -uy;
        let perp_y = ux;

        let h1_x = base_x + perp_x * head_half_width;
        let h1_y = base_y + perp_y * head_half_width;
        let h2_x = base_x - perp_x * head_half_width;
        let h2_y = base_y - perp_y * head_half_width;

        path.move_to(kurbo::Point::new(h1_x, h1_y));
        path.line_to(end);
        path.line_to(kurbo::Point::new(h2_x, h2_y));
        path.close_path();
    }

    let min_x = start.x.min(end.x) - 15.0;
    let max_x = start.x.max(end.x) + 15.0;
    let min_y = start.y.min(end.y) - 15.0;
    let max_y = start.y.max(end.y) + 15.0;
    let bounds = Bounds3D::new_2d(min_x, min_y, max_x, max_y);

    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(Some(gaanim_core::peniko::Brush::Solid(
        gaanim_core::peniko::Color::WHITE,
    )));
    bundle.stroke = StrokeBrush {
        brush: Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        )),
        style: kurbo::Stroke::new(2.5),
    };
    bundle.tag = ObjectTag("Arrow".into());
    bundle
}
