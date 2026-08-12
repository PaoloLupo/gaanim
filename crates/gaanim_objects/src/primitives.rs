use bevy::prelude::*;
use gaanim_core::ObjectId;
use gaanim_core::kurbo::{self, Shape};
use gaanim_math::{Bounds3D, GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{
    FillBrush, GlobalOpacity, LocalBounds, MobjectId, ObjectTag, Opacity, Path2D, PathSource,
    RasterImage, RenderLayer, RenderOrder, StrokeBrush, Visible,
};

/// The source pixels and destination geometry for an image mobject.
///
/// Source coordinates use the conventional top-left/Y-down pixel space. The
/// destination is expressed in gaanim scene units and remains centred on the
/// mobject origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImageView {
    pub source_x: f64,
    pub source_y: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub display_width: f64,
    pub display_height: f64,
    pub scale_x: f64,
    pub scale_y: f64,
}

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
    pub bevy_transform: Transform,
    pub bevy_visibility: Visibility,
    pub fill: FillBrush,
    pub stroke: StrokeBrush,
    pub opacity: Opacity,
    pub global_opacity: GlobalOpacity,
    pub render_order: RenderOrder,
    pub render_layer: RenderLayer,
    pub visible: Visible,
    pub tag: ObjectTag,
    pub raster_image: RasterImage,
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
            bevy_transform: Transform::default(),
            bevy_visibility: Visibility::default(),
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
            raster_image: RasterImage::none(),
        }
    }
}

/// Creates a raster image mobject with its visual center at the local origin.
///
/// One scene unit maps to one source pixel before a caller applies `.scaled()`.
/// The decoded pixels are held in `RasterImage`, so the renderer can issue a
/// native Vello image draw rather than approximating the texture as a vector
/// fill.
pub fn image(
    id: ObjectId,
    image: gaanim_core::peniko::ImageData,
    view: ImageView,
) -> MobjectBundle {
    let w2 = view.display_width * 0.5;
    let h2 = view.display_height * 0.5;
    let path = kurbo::Rect::new(-w2, -h2, w2, h2).to_path(0.1);
    let bounds = Bounds3D::new_2d(-w2, -h2, w2, h2);
    let mut bundle = MobjectBundle::new(id, path, bounds);
    bundle.fill = FillBrush::transparent();
    bundle.tag = ObjectTag("ImageMobject".into());
    bundle.raster_image = RasterImage::new(
        gaanim_core::peniko::ImageBrush::new(image),
        // Image pixels are top-left/Y-down; gaanim mobjects are centred/Y-up.
        kurbo::Affine::new([
            view.scale_x,
            0.0,
            0.0,
            -view.scale_y,
            -view.source_x * view.scale_x - w2,
            view.source_y * view.scale_y + h2,
        ]),
    );
    bundle
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

/// Builds an open helical spring between two points.
///
/// The endpoints remain exact even when the spring is rotated or compressed.
/// `coils` controls the number of turns and `amplitude` is measured
/// perpendicular to the spring axis. The coil is represented by a sampled
/// sinusoid, so changing the endpoint distance changes its pitch while the
/// radius remains stable. This is what makes a reactive spring visibly deform
/// when one of its endpoints is animated.
///
/// `start_straight` and `end_straight` reserve straight scene-unit segments at
/// the corresponding endpoints before the coils begin and after they end. If
/// their combined length exceeds the endpoint distance, both are shortened
/// proportionally and the path becomes a straight line.
///
/// `crossing` is a normalized visual interlacing amount. At `0.0` each turn is
/// a regular sinusoidal coil; at `1.0` the turn briefly folds back along its
/// axis, producing an e-like crossover without moving either endpoint.
pub fn spring_path(
    start: kurbo::Point,
    end: kurbo::Point,
    coils: usize,
    amplitude: f64,
    crossing: f64,
    start_straight: f64,
    end_straight: f64,
) -> kurbo::BezPath {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    let mut path = kurbo::BezPath::new();
    path.move_to(start);

    if length <= f64::EPSILON {
        path.line_to(end);
        return path;
    }

    let direction = (dx / length, dy / length);
    let normal = (-direction.1, direction.0);
    let sanitize_straight = |value: f64| value.is_finite().then_some(value.max(0.0)).unwrap_or(0.0);
    let mut start_straight = sanitize_straight(start_straight);
    let mut end_straight = sanitize_straight(end_straight);
    let requested_straight = start_straight + end_straight;
    if requested_straight > length {
        let scale = length / requested_straight;
        start_straight *= scale;
        end_straight *= scale;
    }
    let coil_length = length - start_straight - end_straight;
    if coil_length <= f64::EPSILON {
        path.line_to(end);
        return path;
    }
    let coil_start = kurbo::Point::new(
        start.x + direction.0 * start_straight,
        start.y + direction.1 * start_straight,
    );
    let coil_end = kurbo::Point::new(
        end.x - direction.0 * end_straight,
        end.y - direction.1 * end_straight,
    );
    if start_straight > f64::EPSILON {
        path.line_to(coil_start);
    }
    let turns = coils.max(1);
    let turns_f64 = turns as f64;
    let crossing = crossing.clamp(0.0, 1.0);
    // A fixed number of samples per turn keeps the projected helix smooth at
    // different lengths. The cap prevents pathological input from producing
    // an unbounded path while preserving the requested endpoints.
    let samples = turns.saturating_mul(24).clamp(24, 4096);
    for index in 1..samples {
        let t = index as f64 / samples as f64;
        let turn_position = t * turns_f64;
        let turn_index = turn_position.floor();
        let turn_t = turn_position - turn_index;
        let phase = std::f64::consts::TAU * turn_t;
        // The cosine term is zero at both ends of every turn. Its bounded axial
        // excursion makes the high-crossing variant loop back over part of the
        // preceding coil while keeping the spring's endpoints exact.
        let axial_t = (turn_index + turn_t + crossing * 0.35 * (phase.cos() - 1.0)) / turns_f64;
        let offset = amplitude * phase.sin();
        path.line_to(kurbo::Point::new(
            coil_start.x + direction.0 * coil_length * axial_t + normal.0 * offset,
            coil_start.y + direction.1 * coil_length * axial_t + normal.1 * offset,
        ));
    }
    path.line_to(coil_end);
    if end_straight > f64::EPSILON {
        path.line_to(end);
    }
    path
}

/// Builds a filled technical-dimension silhouette from three thin line quads
/// and two solid triangular arrowheads.
pub fn dimension_path(start: kurbo::Point, end: kurbo::Point, offset: f64) -> kurbo::BezPath {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let length = dx.hypot(dy);
    let mut path = kurbo::BezPath::new();
    if length <= f64::EPSILON {
        return path;
    }

    let direction = (dx / length, dy / length);
    let normal = (-direction.1, direction.0);
    let dimension_start =
        kurbo::Point::new(start.x + normal.0 * offset, start.y + normal.1 * offset);
    let dimension_end = kurbo::Point::new(end.x + normal.0 * offset, end.y + normal.1 * offset);
    let head = (length * 0.12).clamp(6.0, 12.0).min(length * 0.45);
    let wing = head * 0.55;
    let add_line_quad = |path: &mut kurbo::BezPath, from: kurbo::Point, to: kurbo::Point| {
        let segment = to - from;
        let segment_length = segment.hypot();
        if segment_length <= f64::EPSILON {
            return;
        }
        let half_width = 1.0;
        let nx = -segment.y / segment_length * half_width;
        let ny = segment.x / segment_length * half_width;
        path.move_to(kurbo::Point::new(from.x + nx, from.y + ny));
        path.line_to(kurbo::Point::new(to.x + nx, to.y + ny));
        path.line_to(kurbo::Point::new(to.x - nx, to.y - ny));
        path.line_to(kurbo::Point::new(from.x - nx, from.y - ny));
        path.close_path();
    };
    add_line_quad(&mut path, start, dimension_start);
    add_line_quad(&mut path, end, dimension_end);
    add_line_quad(
        &mut path,
        kurbo::Point::new(
            dimension_start.x + direction.0 * head,
            dimension_start.y + direction.1 * head,
        ),
        kurbo::Point::new(
            dimension_end.x - direction.0 * head,
            dimension_end.y - direction.1 * head,
        ),
    );
    let add_head = |path: &mut kurbo::BezPath, tip: kurbo::Point, sign: f64| {
        let back = kurbo::Point::new(
            tip.x + direction.0 * head * sign,
            tip.y + direction.1 * head * sign,
        );
        path.move_to(tip);
        path.line_to(kurbo::Point::new(
            back.x + normal.0 * wing,
            back.y + normal.1 * wing,
        ));
        path.line_to(kurbo::Point::new(
            back.x - normal.0 * wing,
            back.y - normal.1 * wing,
        ));
        path.close_path();
    };
    add_head(&mut path, dimension_start, 1.0);
    add_head(&mut path, dimension_end, -1.0);
    path
}

/// Creates a curved arrow between two points using a signed angular deflection.
/// Its fill is kept inside a narrow, closed shaft silhouette.
pub fn curved_arrow(
    id: ObjectId,
    start: kurbo::Point,
    end: kurbo::Point,
    angle: f64,
) -> MobjectBundle {
    let dx = end.x - start.x;
    let dy = end.y - start.y;
    let chord = (dx * dx + dy * dy).sqrt();
    if chord <= f64::EPSILON || angle.abs() <= 1e-6 {
        return arrow(id, start, end);
    }

    let radius = (chord * 0.5) / (angle * 0.5).sin().abs();
    let r_sign = angle.signum();
    let h = (radius * radius - chord * chord * 0.25).max(0.0).sqrt();
    let center = kurbo::Point::new(
        (start.x + end.x) * 0.5 + (-dy / chord) * h * r_sign,
        (start.y + end.y) * 0.5 + (dx / chord) * h * r_sign,
    );

    let sa = (start.y - center.y).atan2(start.x - center.x);
    let ea = (end.y - center.y).atan2(end.x - center.x);
    let mut sweep = ea - sa;
    if angle > 0.0 && sweep < 0.0 {
        sweep += 2.0 * std::f64::consts::PI;
    } else if angle < 0.0 && sweep > 0.0 {
        sweep -= 2.0 * std::f64::consts::PI;
    }

    curved_arrow_arc(id, center, radius, sa, sweep)
}

/// Creates a curved arrow from an explicit circular arc.
///
/// `start_angle` and `sweep_angle` are expressed in radians. The arrow tip is
/// placed at the end of the sweep, so the center and radius can be shared with
/// another object (for example, a rotating disk or a tracked point).
pub fn curved_arrow_arc(
    id: ObjectId,
    center: kurbo::Point,
    radius: f64,
    start_angle: f64,
    sweep_angle: f64,
) -> MobjectBundle {
    let radius = radius.abs();
    let sa = start_angle;
    let start = center + kurbo::Vec2::new(radius * start_angle.cos(), radius * start_angle.sin());
    let end_angle = start_angle + sweep_angle;
    let end = center + kurbo::Vec2::new(radius * end_angle.cos(), radius * end_angle.sin());

    if radius <= f64::EPSILON || sweep_angle.abs() <= f64::EPSILON {
        let mut bundle = MobjectBundle::new(
            id,
            kurbo::BezPath::new(),
            Bounds3D::new_2d(start.x, start.y, start.x, start.y),
        );
        bundle.tag = ObjectTag("CurvedArrow".into());
        return bundle;
    }

    let head_len: f64 = 18.0;
    // Match the default 2.5px stroke so applying a fill does not make the
    // shaft visibly wider than its outline.
    let body_half_t: f64 = 1.25;
    // Keep the inner shoulder on the same side of the center as the shaft;
    // this avoids an oversized/inverted fill for small-radius arcs.
    let head_half_width: f64 = 9.0_f64.min((radius * 0.45).max(body_half_t));

    let sweep = sweep_angle;
    let sweep_sign = sweep.signum();
    let sweep_abs = sweep.abs();
    let head_angle = (head_len / radius).min(sweep_abs * 0.5);
    let shaft_sweep = (sweep_abs - head_angle).max(0.0);
    let sa_shoulder = end_angle - sweep_sign * head_angle;

    let mut path = kurbo::BezPath::new();

    // The shaft is a thin closed ribbon. Vector renderers implicitly close
    // open subpaths for filling, which would otherwise turn a large arc into
    // a filled circular sector.
    let r_outer = radius + body_half_t;
    let r_inner = (radius - body_half_t).max(0.0);
    path.move_to(center + kurbo::Vec2::new(r_outer * sa.cos(), r_outer * sa.sin()));

    let steps = ((radius * shaft_sweep / 4.0).ceil() as u32).max(8);
    for i in 0..=steps {
        let a = sa + sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        path.line_to(center + kurbo::Vec2::new(r_outer * a.cos(), r_outer * a.sin()));
    }

    // The arrowhead tip lies exactly on the requested arc.
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
    path.line_to(
        center + kurbo::Vec2::new(r_inner * sa_shoulder.cos(), r_inner * sa_shoulder.sin()),
    );

    for i in 0..=steps {
        let a = sa_shoulder - sweep_sign * shaft_sweep * (i as f64 / steps as f64);
        path.line_to(center + kurbo::Vec2::new(r_inner * a.cos(), r_inner * a.sin()));
    }
    path.close_path();

    let bounds_rect = path.bounding_box();
    let bounds = Bounds3D::new_2d(
        bounds_rect.x0 - body_half_t,
        bounds_rect.y0 - body_half_t,
        bounds_rect.x1 + body_half_t,
        bounds_rect.y1 + body_half_t,
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

    #[test]
    fn curved_arrow_arc_fill_stays_inside_narrow_silhouette() {
        let b = curved_arrow_arc(
            ObjectId::from_raw(0),
            kurbo::Point::new(0.0, 0.0),
            100.0,
            0.0,
            std::f64::consts::FRAC_PI_2,
        );
        assert_eq!(
            count_subpaths(&b.path.0),
            1,
            "the arrow must be a single closed silhouette, never an open arc that fills as a sector"
        );
        assert_eq!(
            b.path.0.winding(kurbo::Point::new(0.0, 0.0)),
            0,
            "the circle center must stay outside the filled arrow body"
        );
    }

    #[test]
    fn curved_arrow_arc_tip_stays_on_requested_radius() {
        let center = kurbo::Point::new(10.0, -4.0);
        let radius = 80.0;
        let start_angle = 0.2;
        let sweep = 1.1;
        let b = curved_arrow_arc(ObjectId::from_raw(0), center, radius, start_angle, sweep);
        let expected = center
            + kurbo::Vec2::new(
                radius * (start_angle + sweep).cos(),
                radius * (start_angle + sweep).sin(),
            );
        assert!(b.path.0.elements().iter().any(|element| {
            matches!(element, kurbo::PathEl::LineTo(point) if point.distance(expected) < 1e-6)
        }));
    }

    #[test]
    fn spring_path_keeps_its_endpoints_and_deforms_as_a_helical_coil() {
        let start = kurbo::Point::new(-80.0, 10.0);
        let compact_end = kurbo::Point::new(120.0, 10.0);
        let stretched_end = kurbo::Point::new(320.0, 10.0);
        let compact = spring_path(start, compact_end, 4, 15.0, 0.0, 20.0, 30.0);
        let stretched = spring_path(start, stretched_end, 4, 15.0, 0.0, 20.0, 30.0);
        let compact_points: Vec<_> = compact
            .elements()
            .iter()
            .filter_map(|element| match element {
                kurbo::PathEl::LineTo(point) => Some(*point),
                _ => None,
            })
            .collect();
        let stretched_points: Vec<_> = stretched
            .elements()
            .iter()
            .filter_map(|element| match element {
                kurbo::PathEl::LineTo(point) => Some(*point),
                _ => None,
            })
            .collect();

        assert!(
            matches!(compact.elements().first(), Some(kurbo::PathEl::MoveTo(p)) if *p == start)
        );
        assert!(
            matches!(compact.elements().last(), Some(kurbo::PathEl::LineTo(p)) if *p == compact_end)
        );
        assert!(
            matches!(stretched.elements().last(), Some(kurbo::PathEl::LineTo(p)) if *p == stretched_end)
        );
        assert_eq!(compact_points.len(), stretched_points.len());
        assert!(
            compact_points
                .iter()
                .any(|point| (point.y - start.y).abs() > 1e-6),
            "a helical spring with amplitude must deviate from its axis"
        );
        let peak = compact_points
            .iter()
            .map(|point| (point.y - start.y).abs())
            .fold(0.0_f64, f64::max);
        assert!((peak - 15.0).abs() < 1e-6);
        assert!(compact_points[6].x < stretched_points[6].x);

        let interlaced = spring_path(start, compact_end, 4, 15.0, 1.0, 20.0, 30.0);
        let interlaced_points: Vec<_> = interlaced
            .elements()
            .iter()
            .filter_map(|element| match element {
                kurbo::PathEl::LineTo(point) => Some(*point),
                _ => None,
            })
            .collect();
        assert!(
            compact_points
                .windows(2)
                .all(|points| points[1].x >= points[0].x),
            "a regular spring should progress monotonically along its axis"
        );
        assert!(
            interlaced_points
                .windows(2)
                .any(|points| points[1].x < points[0].x),
            "crossing should fold part of a turn back along the axis"
        );
        assert!(matches!(
            interlaced.elements().last(),
            Some(kurbo::PathEl::LineTo(p)) if *p == compact_end
        ));
        assert!(matches!(
            compact.elements().get(1),
            Some(kurbo::PathEl::LineTo(p)) if *p == kurbo::Point::new(-60.0, 10.0)
        ));
        assert!(
            compact_points
                .iter()
                .rev()
                .nth(1)
                .is_some_and(|point| *point == kurbo::Point::new(90.0, 10.0)),
            "the requested end straight must start where the final coil ends"
        );
    }

    #[test]
    fn dimension_path_contains_extensions_and_arrowheads() {
        let path = dimension_path(
            kurbo::Point::new(-60.0, 0.0),
            kurbo::Point::new(60.0, 0.0),
            30.0,
        );
        assert_eq!(
            path.elements()
                .iter()
                .filter(|element| matches!(element, kurbo::PathEl::ClosePath))
                .count(),
            5,
            "three line quads and both arrowheads must be closed fill geometry"
        );
        assert!(path.elements().iter().any(|element| {
            matches!(element, kurbo::PathEl::LineTo(point) if (point.y - 30.0).abs() < 1e-6)
        }));
    }
}
