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

/// Creates a MobjectBundle from a list of polylines (closed subpaths).
/// Multiple polylines are interpreted as outer rings (first) and holes
/// (rest) using the even-odd fill rule. Used by the boolean-operation
/// replay path.
pub fn polylines(id: ObjectId, rings: &[Vec<kurbo::Point>]) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for ring in rings {
        if ring.is_empty() {
            continue;
        }
        path.move_to(ring[0]);
        for &p in &ring[1..] {
            path.line_to(p);
        }
        path.close_path();
        for &p in ring {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
    }
    let bounds = if rings.is_empty() {
        Bounds3D::default()
    } else {
        Bounds3D::new_2d(min_x, min_y, max_x, max_y)
    };
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("BooleanResult".into());
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

/// Creates a directional arrow Mobject bundle with a solid filled body
/// and a triangular head.
///
/// The geometry is built as a **single closed subpath shaped like a
/// real arrow** (a rectangular body capped by a wider triangular head):
///
/// ```text
///        shoulder_top ──────── h1
///       /                              \
///   start_back                       tip
///       \                              /
///        shoulder_bot ──────── h2
/// ```
///
/// `start_back` is the (perpendicular) back of the body at `start`;
/// `shoulder_top` / `shoulder_bot` are where the body meets the head
/// on the top/bottom; `h1` / `h2` are the wider shoulders of the head;
/// `tip` is `end`. The body has non-zero area, so the fill covers both
/// the body and the head — the tail never disappears.
///
/// `PathCompletion 0->1` (used by `Write`/`Create`/`GrowArrow`) reveals
/// the path as a continuous pen stroke: top of body, top of head, tip,
/// bottom of head, bottom of body, then closes around the tail.
pub fn arrow(id: ObjectId, start: kurbo::Point, end: kurbo::Point) -> MobjectBundle {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    let head_len: f64 = 18.0;
    let head_half_width: f64 = 9.0;
    let body_half_t: f64 = 3.0;

    let mut path = kurbo::BezPath::new();
    if len > 0.0 {
        let ux = dx / len;
        let uy = dy / len;
        let perp_x = -uy;
        let perp_y = ux;

        let base_x = end.x - ux * head_len;
        let base_y = end.y - uy * head_len;

        let start_top_x = start.x - perp_x * body_half_t;
        let start_top_y = start.y - perp_y * body_half_t;
        let start_bot_x = start.x + perp_x * body_half_t;
        let start_bot_y = start.y + perp_y * body_half_t;

        let shoulder_top_x = base_x - perp_x * body_half_t;
        let shoulder_top_y = base_y - perp_y * body_half_t;
        let shoulder_bot_x = base_x + perp_x * body_half_t;
        let shoulder_bot_y = base_y + perp_y * body_half_t;

        let h1_x = base_x - perp_x * head_half_width;
        let h1_y = base_y - perp_y * head_half_width;
        let h2_x = base_x + perp_x * head_half_width;
        let h2_y = base_y + perp_y * head_half_width;

        // Single closed subpath: pentagonal arrow silhouette.
        // The fill covers the whole shape so the body is just as
        // visible as the head after PathCompletion reaches 1.0.
        path.move_to(kurbo::Point::new(start_top_x, start_top_y));
        path.line_to(kurbo::Point::new(shoulder_top_x, shoulder_top_y));
        path.line_to(kurbo::Point::new(h1_x, h1_y));
        path.line_to(end);
        path.line_to(kurbo::Point::new(h2_x, h2_y));
        path.line_to(kurbo::Point::new(shoulder_bot_x, shoulder_bot_y));
        path.line_to(kurbo::Point::new(start_bot_x, start_bot_y));
        path.close_path();
    }

    let pad = head_half_width.max(body_half_t);
    let min_x = start.x.min(end.x) - pad;
    let max_x = start.x.max(end.x) + pad;
    let min_y = start.y.min(end.y) - pad;
    let max_y = start.y.max(end.y) + pad;
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

pub fn dashed_line(
    id: ObjectId,
    start: kurbo::Point,
    end: kurbo::Point,
    dash_length: f64,
    gap_length: f64,
) -> MobjectBundle {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();
    if len > 0.0 {
        let ux = dx / len;
        let uy = dy / len;
        let mut travelled = 0.0;
        let mut drawing = true;
        while travelled < len {
            let seg_len = if drawing { dash_length } else { gap_length };
            let effective = (travelled + seg_len).min(len) - travelled;
            if drawing && effective > 0.0 {
                let sx = start.x + ux * travelled;
                let sy = start.y + uy * travelled;
                let ex = sx + ux * effective;
                let ey = sy + uy * effective;
                path.move_to(kurbo::Point::new(sx, sy));
                path.line_to(kurbo::Point::new(ex, ey));
            }
            travelled += seg_len;
            drawing = !drawing;
        }
    }

    let min_x = start.x.min(end.x);
    let max_x = start.x.max(end.x);
    let min_y = start.y.min(end.y);
    let max_y = start.y.max(end.y);
    let bounds = Bounds3D::new_2d(min_x, min_y, max_x, max_y);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(None);
    bundle.tag = ObjectTag("DashedLine".into());
    bundle
}

pub fn arc_between_points(
    id: ObjectId,
    start: kurbo::Point,
    end: kurbo::Point,
    angle: f64,
) -> MobjectBundle {
    let mid_x = (start.x + end.x) * 0.5;
    let mid_y = (start.y + end.y) * 0.5;
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = (dx * dx + dy * dy).sqrt();
    let radius = if angle.abs() < 1e-6 {
        chord * 0.5
    } else {
        (chord * 0.5) / (angle * 0.5).sin().abs()
    };
    let r_sign = if angle >= 0.0 { 1.0 } else { -1.0 };
    let h = (radius * radius - chord * chord * 0.25).sqrt();
    let nx = -dy / chord;
    let ny = dx / chord;
    let cx = mid_x + nx * h * r_sign;
    let cy = mid_y + ny * h * r_sign;

    let sa = (start.y - cy).atan2(start.x - cx);
    let ea = (end.y - cy).atan2(end.x - cx);
    let mut sweep = ea - sa;
    if angle > 0.0 && sweep < 0.0 {
        sweep += 2.0 * std::f64::consts::PI;
    } else if angle < 0.0 && sweep > 0.0 {
        sweep -= 2.0 * std::f64::consts::PI;
    }

    let mut bundle = arc(
        id,
        kurbo::Point::new(cx, cy),
        kurbo::Vec2::new(radius, radius),
        sa,
        sweep,
        0.0,
    );
    bundle.tag = ObjectTag("ArcBetweenPoints".into());
    bundle
}

pub fn double_arrow(
    id: ObjectId,
    start: kurbo::Point,
    end: kurbo::Point,
    head_len: Option<f64>,
    head_width: Option<f64>,
) -> MobjectBundle {
    let mut head_len = head_len.unwrap_or(18.0);
    let head_half_width = head_width.unwrap_or(18.0) * 0.5;
    let body_half_t: f64 = 3.0;

    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();

    let mut path = kurbo::BezPath::new();
    if len > 0.0 {
        let ux = dx / len;
        let uy = dy / len;
        let perp_x = -uy;
        let perp_y = ux;

        // Cap head_len so the two heads never overlap.
        head_len = head_len.min(len * 0.4);

        let base_start_x = start.x + ux * head_len;
        let base_start_y = start.y + uy * head_len;
        let base_end_x = end.x - ux * head_len;
        let base_end_y = end.y - uy * head_len;

        // Start head (tip at `start`)
        let h1s_x = base_start_x - perp_x * head_half_width;
        let h1s_y = base_start_y - perp_y * head_half_width;
        let h2s_x = base_start_x + perp_x * head_half_width;
        let h2s_y = base_start_y + perp_y * head_half_width;

        // End head (tip at `end`)
        let h1e_x = base_end_x - perp_x * head_half_width;
        let h1e_y = base_end_y - perp_y * head_half_width;
        let h2e_x = base_end_x + perp_x * head_half_width;
        let h2e_y = base_end_y + perp_y * head_half_width;

        // Body shoulders (where each head meets the body on each side).
        let shoulder_top_start_x = base_start_x - perp_x * body_half_t;
        let shoulder_top_start_y = base_start_y - perp_y * body_half_t;
        let shoulder_bot_start_x = base_start_x + perp_x * body_half_t;
        let shoulder_bot_start_y = base_start_y + perp_y * body_half_t;
        let shoulder_top_end_x = base_end_x - perp_x * body_half_t;
        let shoulder_top_end_y = base_end_y - perp_y * body_half_t;
        let shoulder_bot_end_x = base_end_x + perp_x * body_half_t;
        let shoulder_bot_end_y = base_end_y + perp_y * body_half_t;

        // Single closed subpath shaped like a double-headed arrow.
        // The fill covers the whole silhouette (body + both heads) so
        // the body tail never disappears after GrowArrow completes.
        // Order: start head top -> tip -> start head bottom ->
        //        body bottom -> end head bottom -> end tip -> end
        //        head top -> body top -> back to start.
        path.move_to(kurbo::Point::new(h1s_x, h1s_y));
        path.line_to(start);
        path.line_to(kurbo::Point::new(h2s_x, h2s_y));
        path.line_to(kurbo::Point::new(
            shoulder_bot_start_x,
            shoulder_bot_start_y,
        ));
        path.line_to(kurbo::Point::new(shoulder_bot_end_x, shoulder_bot_end_y));
        path.line_to(kurbo::Point::new(h2e_x, h2e_y));
        path.line_to(end);
        path.line_to(kurbo::Point::new(h1e_x, h1e_y));
        path.line_to(kurbo::Point::new(shoulder_top_end_x, shoulder_top_end_y));
        path.line_to(kurbo::Point::new(
            shoulder_top_start_x,
            shoulder_top_start_y,
        ));
        path.close_path();
    }

    let pad = head_half_width.max(body_half_t);
    let min_x = start.x.min(end.x) - pad;
    let max_x = start.x.max(end.x) + pad;
    let min_y = start.y.min(end.y) - pad;
    let max_y = start.y.max(end.y) + pad;
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
    bundle.tag = ObjectTag("DoubleArrow".into());
    bundle
}

pub fn sector(
    id: ObjectId,
    center: kurbo::Point,
    radius: f64,
    start_angle: f64,
    sweep_angle: f64,
) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    path.move_to(center);
    let perimeter = radius * sweep_angle.abs();
    let max_chord = 4.0_f64;
    let steps = ((perimeter / max_chord).ceil() as u32).max(8);
    for i in 0..=steps {
        let a = start_angle + sweep_angle * (i as f64 / steps as f64);
        let x = center.x + radius * a.cos();
        let y = center.y + radius * a.sin();
        path.line_to(kurbo::Point::new(x, y));
    }
    path.close_path();
    let bounds = Bounds3D::new_2d(
        center.x - radius,
        center.y - radius,
        center.x + radius,
        center.y + radius,
    );
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Sector".into());
    bundle
}

pub fn annulus(id: ObjectId, outer_radius: f64, inner_radius: f64) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    let perimeter = 2.0 * std::f64::consts::PI * outer_radius;
    let max_chord = 4.0_f64;
    let steps = ((perimeter / max_chord).ceil() as u32).max(16);
    for i in 0..=steps {
        let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
        let x = outer_radius * a.cos();
        let y = outer_radius * a.sin();
        if i == 0 {
            path.move_to(kurbo::Point::new(x, y));
        } else {
            path.line_to(kurbo::Point::new(x, y));
        }
    }
    path.close_path();
    for i in 0..=steps {
        let a = i as f64 * 2.0 * std::f64::consts::PI / steps as f64;
        let x = inner_radius * a.cos();
        let y = inner_radius * a.sin();
        if i == 0 {
            path.move_to(kurbo::Point::new(x, y));
        } else {
            path.line_to(kurbo::Point::new(x, y));
        }
    }
    path.close_path();
    let bounds = Bounds3D::new_2d(-outer_radius, -outer_radius, outer_radius, outer_radius);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.tag = ObjectTag("Annulus".into());
    bundle
}

pub fn surrounding_rectangle(
    id: ObjectId,
    width: f64,
    height: f64,
    corner_radius: f64,
) -> MobjectBundle {
    let w2 = width / 2.0;
    let h2 = height / 2.0;
    let path = if corner_radius > 0.0 {
        kurbo::RoundedRect::new(-w2, -h2, w2, h2, corner_radius).to_path(0.1)
    } else {
        kurbo::Rect::new(-w2, -h2, w2, h2).to_path(0.1)
    };
    let bounds = Bounds3D::new_2d(-w2, -h2, w2, h2);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(None);
    bundle.tag = ObjectTag("SurroundingRectangle".into());
    bundle
}

pub fn background_rectangle(id: ObjectId, width: f64, height: f64) -> MobjectBundle {
    let mut bundle = rectangle(id, width, height);
    bundle.tag = ObjectTag("BackgroundRectangle".into());
    bundle.render_order = RenderOrder {
        z_index: -10,
        creation_order: id.index() as u64,
    };
    bundle
}

pub fn cross(id: ObjectId, size: f64) -> MobjectBundle {
    let h = size * 0.5;
    let mut path = kurbo::BezPath::new();
    path.move_to(kurbo::Point::new(-h, -h));
    path.line_to(kurbo::Point::new(h, h));
    path.move_to(kurbo::Point::new(h, -h));
    path.line_to(kurbo::Point::new(-h, h));
    let bounds = Bounds3D::new_2d(-h, -h, h, h);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(None);
    bundle.tag = ObjectTag("Cross".into());
    bundle
}

pub fn right_angle(id: ObjectId, arm_length: f64) -> MobjectBundle {
    // The corner sits at the origin; arms extend along +x and +y.
    let mut path = kurbo::BezPath::new();
    path.move_to(kurbo::Point::new(0.0, 0.0));
    path.line_to(kurbo::Point::new(arm_length, 0.0));
    path.line_to(kurbo::Point::new(0.0, 0.0));
    path.line_to(kurbo::Point::new(0.0, arm_length));
    let bounds = Bounds3D::new_2d(0.0, 0.0, arm_length, arm_length);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(None);
    bundle.tag = ObjectTag("RightAngle".into());
    bundle
}

/// Creates a line mobject that is tangent to a polyline curve at a
/// fractional position `t` in `[0.0, 1.0]` along the curve.
///
/// `curve` is a list of world-space points; adjacent points are
/// connected by line segments. The tangent at `t` is the direction of
/// the segment that contains `t` (interior points use the direction of
/// the segment whose cumulative arc length includes `t`).
///
/// `length` is the half-length of the tangent line (the line extends
/// ±`length` from the tangent point, in the local tangent direction).
pub fn tangent_line(
    id: ObjectId,
    curve: &[kurbo::Point],
    t: f64,
    length: f64,
) -> Option<MobjectBundle> {
    if curve.len() < 2 {
        return None;
    }

    // Compute segment lengths to find the segment containing `t`.
    let seg_lengths: Vec<f64> = curve
        .windows(2)
        .map(|w| {
            let dx = w[1].x - w[0].x;
            let dy = w[1].y - w[0].y;
            (dx * dx + dy * dy).sqrt()
        })
        .collect();
    let total_length: f64 = seg_lengths.iter().sum();
    if total_length <= 0.0 {
        return None;
    }

    let t_clamped = t.clamp(0.0, 1.0);
    let target = t_clamped * total_length;

    let mut cumulative = 0.0;
    let mut idx = 0;
    let mut local_t = 0.0;
    for (i, &seg) in seg_lengths.iter().enumerate() {
        if cumulative + seg >= target || i == seg_lengths.len() - 1 {
            idx = i;
            local_t = if seg > 0.0 {
                (target - cumulative) / seg
            } else {
                0.0
            };
            break;
        }
        cumulative += seg;
    }

    let p0 = curve[idx];
    let p1 = curve[idx + 1];
    let tangent_point = kurbo::Point::new(
        p0.x + (p1.x - p0.x) * local_t,
        p0.y + (p1.y - p0.y) * local_t,
    );
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let mag = (dx * dx + dy * dy).sqrt();
    if mag <= 0.0 {
        return None;
    }
    let ux = dx / mag;
    let uy = dy / mag;

    let start = kurbo::Point::new(tangent_point.x - ux * length, tangent_point.y - uy * length);
    let end = kurbo::Point::new(tangent_point.x + ux * length, tangent_point.y + uy * length);

    Some(line(id, start, end))
}

/// Creates a Cartesian number plane mobject: x-axis, y-axis, and an
/// optional grid of equally-spaced lines.
///
/// `x_range`, `y_range`: `(min, max, step)` tuples defining axis extents
/// and grid spacing. `axis_stroke` and `grid_stroke` control the
/// thickness; the axes are drawn thicker than the grid.
pub fn number_plane(
    id: ObjectId,
    x_range: (f64, f64, f64),
    y_range: (f64, f64, f64),
    axis_stroke: f64,
    grid_stroke: f64,
) -> MobjectBundle {
    let (x_min, x_max, x_step) = x_range;
    let (y_min, y_max, y_step) = y_range;

    let mut path = kurbo::BezPath::new();

    // Grid lines (vertical)
    let mut x = (x_min / x_step).ceil() * x_step;
    while x <= x_max + 1e-9 {
        path.move_to(kurbo::Point::new(x, y_min));
        path.line_to(kurbo::Point::new(x, y_max));
        x += x_step;
    }
    // Grid lines (horizontal)
    let mut y = (y_min / y_step).ceil() * y_step;
    while y <= y_max + 1e-9 {
        path.move_to(kurbo::Point::new(x_min, y));
        path.line_to(kurbo::Point::new(x_max, y));
        y += y_step;
    }

    // Axes (thicker) — drawn last to overlay the grid
    path.move_to(kurbo::Point::new(x_min, 0.0));
    path.line_to(kurbo::Point::new(x_max, 0.0));
    path.move_to(kurbo::Point::new(0.0, y_min));
    path.line_to(kurbo::Point::new(0.0, y_max));

    let bounds = Bounds3D::new_2d(x_min, y_min, x_max, y_max);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush(None);
    bundle.stroke = StrokeBrush {
        brush: Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::from_rgb8(0xA0, 0xA0, 0xA0),
        )),
        style: kurbo::Stroke::new(grid_stroke),
    };
    bundle.tag = ObjectTag("NumberPlane".into());
    let _ = axis_stroke; // axes use a thicker stroke applied by the caller via mobject.stroke()
    bundle
}

pub fn open_path(id: ObjectId, points: &[kurbo::Point]) -> MobjectBundle {
    let mut path = kurbo::BezPath::new();
    if !points.is_empty() {
        path.move_to(points[0]);
        for &p in &points[1..] {
            path.line_to(p);
        }
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
    bundle.fill = FillBrush(None);
    bundle.stroke = StrokeBrush {
        brush: Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        )),
        style: kurbo::Stroke::new(2.0),
    };
    bundle.tag = ObjectTag("OpenPath".into());
    bundle
}

pub fn curved_arrow(
    id: ObjectId,
    start: kurbo::Point,
    end: kurbo::Point,
    angle: f64,
) -> MobjectBundle {
    let mid_x = (start.x + end.x) * 0.5;
    let mid_y = (start.y + end.y) * 0.5;
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = (dx * dx + dy * dy).sqrt();
    let radius = if angle.abs() < 1e-6 {
        chord * 0.5
    } else {
        (chord * 0.5) / (angle * 0.5).sin().abs()
    };
    let r_sign = if angle >= 0.0 { 1.0 } else { -1.0 };
    let h = (radius * radius - chord * chord * 0.25).sqrt();
    let nx = -dy / chord;
    let ny = dx / chord;
    let cx = mid_x + nx * h * r_sign;
    let cy = mid_y + ny * h * r_sign;
    let center = kurbo::Point::new(cx, cy);

    let sa = (start.y - cy).atan2(start.x - cx);
    let ea = (end.y - cy).atan2(end.x - cx);
    let mut sweep = ea - sa;
    if angle > 0.0 && sweep < 0.0 {
        sweep += 2.0 * std::f64::consts::PI;
    } else if angle < 0.0 && sweep > 0.0 {
        sweep -= 2.0 * std::f64::consts::PI;
    }

    let head_len: f64 = 18.0;
    let head_half_width: f64 = 9.0;
    let body_half_t: f64 = 3.0;

    let sweep_sign = sweep.signum();
    let sweep_abs = sweep.abs();
    let head_angle = (head_len / radius).min(sweep_abs * 0.5);
    let shaft_sweep = sweep_abs - head_angle;
    let sa_shoulder = ea - sweep_sign * head_angle;

    let r_outer = radius + body_half_t;
    let r_inner = radius - body_half_t;

    let mut path = kurbo::BezPath::new();

    // Outer arc start
    let p_start_outer = center + kurbo::Vec2::new(r_outer * sa.cos(), r_outer * sa.sin());
    path.move_to(p_start_outer);

    let steps = ((radius * shaft_sweep / 4.0).ceil() as u32).max(8);
    for i in 0..=steps {
        let a = sa + sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        let p = center + kurbo::Vec2::new(r_outer * a.cos(), r_outer * a.sin());
        path.line_to(p);
    }

    // Outer shoulder of the arrow head
    let p_shoulder_outer = center
        + kurbo::Vec2::new(
            (radius + head_half_width) * sa_shoulder.cos(),
            (radius + head_half_width) * sa_shoulder.sin(),
        );
    path.line_to(p_shoulder_outer);

    // Tip
    path.line_to(end);

    // Inner shoulder of the arrow head
    let p_shoulder_inner = center
        + kurbo::Vec2::new(
            (radius - head_half_width) * sa_shoulder.cos(),
            (radius - head_half_width) * sa_shoulder.sin(),
        );
    path.line_to(p_shoulder_inner);

    // Inner shaft shoulder
    let p_shaft_shoulder_inner =
        center + kurbo::Vec2::new(r_inner * sa_shoulder.cos(), r_inner * sa_shoulder.sin());
    path.line_to(p_shaft_shoulder_inner);

    // Inner arc back to start
    for i in 0..=steps {
        let a = sa_shoulder - sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        let p = center + kurbo::Vec2::new(r_inner * a.cos(), r_inner * a.sin());
        path.line_to(p);
    }

    path.close_path();

    let mut min_x = start.x.min(end.x);
    let mut max_x = start.x.max(end.x);
    let mut min_y = start.y.min(end.y);
    let mut max_y = start.y.max(end.y);

    for el in path.elements() {
        if let kurbo::PathEl::LineTo(p) | kurbo::PathEl::MoveTo(p) = el {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
    }

    let bounds = Bounds3D::new_2d(
        min_x - body_half_t.max(head_half_width),
        min_y - body_half_t.max(head_half_width),
        max_x + body_half_t.max(head_half_width),
        max_y + body_half_t.max(head_half_width),
    );

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
    bundle.tag = ObjectTag("CurvedArrow".into());
    bundle
}

pub fn brace(id: ObjectId, start: kurbo::Point, end: kurbo::Point, height: f64) -> MobjectBundle {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let len = (dx * dx + dy * dy).sqrt();
    let theta = dy.atan2(dx);

    let mut path = kurbo::BezPath::new();
    path.move_to(kurbo::Point::new(0.0, 0.0));

    // Segment 1a: (0,0) -> (len/4, -height/2)
    path.curve_to(
        kurbo::Point::new(len / 8.0, 0.0),
        kurbo::Point::new(len / 8.0, -height / 2.0),
        kurbo::Point::new(len / 4.0, -height / 2.0),
    );

    // Segment 1b: (len/4, -height/2) -> (len/2, -height)
    path.curve_to(
        kurbo::Point::new(3.0 * len / 8.0, -height / 2.0),
        kurbo::Point::new(len / 2.0 - len / 16.0, -height),
        kurbo::Point::new(len / 2.0, -height),
    );

    // Segment 2a: (len/2, -height) -> (3*len/4, -height/2)
    path.curve_to(
        kurbo::Point::new(len / 2.0 + len / 16.0, -height),
        kurbo::Point::new(5.0 * len / 8.0, -height / 2.0),
        kurbo::Point::new(3.0 * len / 4.0, -height / 2.0),
    );

    // Segment 2b: (3*len/4, -height/2) -> (len, 0)
    path.curve_to(
        kurbo::Point::new(7.0 * len / 8.0, -height / 2.0),
        kurbo::Point::new(7.0 * len / 8.0, 0.0),
        kurbo::Point::new(len, 0.0),
    );

    let trans = kurbo::Affine::translate((start.x, start.y)) * kurbo::Affine::rotate(theta);
    let final_path = trans * path;

    let bounding_rect = final_path.bounding_box();
    let bounds = Bounds3D::new_2d(
        bounding_rect.x0,
        bounding_rect.y0,
        bounding_rect.x1,
        bounding_rect.y1,
    );

    let mut bundle = MobjectBundle::new(id, final_path, bounds);
    bundle.fill = FillBrush(None);
    bundle.stroke = StrokeBrush {
        brush: Some(gaanim_core::peniko::Brush::Solid(
            gaanim_core::peniko::Color::WHITE,
        )),
        style: kurbo::Stroke::new(2.0),
    };
    bundle.tag = ObjectTag("Brace".into());
    bundle
}

#[cfg(test)]
mod arrow_tests {
    use super::*;
    use gaanim_core::ObjectId;
    use kurbo::Shape;

    fn count_subpaths(path: &kurbo::BezPath) -> usize {
        path.iter()
            .filter(|el| matches!(el, kurbo::PathEl::MoveTo(_)))
            .count()
    }

    #[test]
    fn arrow_uses_single_closed_subpath() {
        let b = arrow(
            ObjectId::from_raw(0),
            kurbo::Point::new(0.0, 0.0),
            kurbo::Point::new(100.0, 0.0),
        );
        assert_eq!(
            count_subpaths(&b.path.0),
            1,
            "arrow must be one continuous subpath so PathCompletion reveals it as a pen stroke"
        );
    }

    #[test]
    fn double_arrow_uses_single_closed_subpath() {
        let b = double_arrow(
            ObjectId::from_raw(0),
            kurbo::Point::new(0.0, 0.0),
            kurbo::Point::new(100.0, 0.0),
            None,
            None,
        );
        assert_eq!(
            count_subpaths(&b.path.0),
            1,
            "double_arrow must be one continuous subpath so the body fill survives GrowArrow"
        );
    }

    #[test]
    fn double_arrow_body_has_non_zero_area() {
        // A horizontal double-arrow: the body rectangle (centered on y=0)
        // must contain interior points well within the silhouette. We
        // sample a point 30% along the body and 0.5px above the axis;
        // a proper filled arrow contains it.
        let b = double_arrow(
            ObjectId::from_raw(0),
            kurbo::Point::new(-50.0, 0.0),
            kurbo::Point::new(50.0, 0.0),
            Some(15.0),
            Some(15.0),
        );
        let sample = kurbo::Point::new(0.0, 0.5);
        assert!(
            b.path.0.winding(sample) != 0,
            "sample point inside the body must be inside the closed silhouette (winding != 0)"
        );
    }

    #[test]
    fn double_arrow_overlapping_heads_caps_head_len() {
        // When the two heads would overlap (head_len > 40% of total
        // length), the geometry should still produce a non-degenerate
        // single closed subpath (no panic, no zero-length body).
        let b = double_arrow(
            ObjectId::from_raw(0),
            kurbo::Point::new(0.0, 0.0),
            kurbo::Point::new(10.0, 0.0),
            Some(50.0),
            Some(20.0),
        );
        assert_eq!(count_subpaths(&b.path.0), 1);
    }
}
