//! Vector geometry of scene transitions and cut overlays.
//!
//! Every function here is a pure function of the transition, its eased
//! progress and the camera frame, so seeks, previews and exports agree
//! exactly. Reveals are `kurbo` paths (plus an optional linear alpha ramp for
//! feathered wipes); overlays are `peniko` fills. Nothing is rasterized.

use std::f64::consts::{PI, TAU};

use bevy::prelude::{ChildOf, Entity, World};
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::{
    Affine, BezPath, Circle, Line, ParamCurveNearest, PathEl, Point, Rect, Shape,
};
use gaanim_core::peniko::{BlendMode, Brush, Color, Compose, Fill, Gradient, Mix};
use gaanim_math::SpatialTransform;
use gaanim_scene::{
    MobjectId, Path2D, SceneTransitionFrame, TransitionMask, TransitionOverlayLayer,
};

use crate::clip::SceneId;
use crate::transition::{IrisShape, SlideDirection, TransitionOverlay, TransitionType};

/// The visible camera frame in which transitions are laid out.
///
/// Geometry is built in *frame space* (origin at the frame center, unrotated,
/// unzoomed, Y-up) and mapped to world space by `view`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransitionView {
    /// Frame space to world space.
    pub view: Affine,
    /// Half of the frame width in frame units.
    pub half_width: f64,
    /// Half of the frame height in frame units.
    pub half_height: f64,
    /// World units per frame unit (the inverse camera zoom).
    pub scale: f64,
}

impl TransitionView {
    /// A frame centered at `center` with the given half extents, rotation
    /// (radians, counter-clockwise) and orthographic zoom.
    pub fn new(center: DVec2, half_width: f64, half_height: f64, rotation: f64, zoom: f64) -> Self {
        let zoom = if zoom.is_finite() && zoom > 0.0 {
            zoom
        } else {
            1.0
        };
        let rotation = if rotation.is_finite() { rotation } else { 0.0 };
        let scale = 1.0 / zoom;
        Self {
            view: Affine::translate((center.x, center.y))
                * Affine::rotate(rotation)
                * Affine::scale(scale),
            half_width: half_width.abs().max(1e-9),
            half_height: half_height.abs().max(1e-9),
            scale,
        }
    }

    /// The frame currently seen by the authored camera.
    pub fn from_camera(camera: &gaanim_math::Camera) -> Self {
        let zoom = match camera.projection {
            gaanim_math::Projection::Orthographic { zoom } => zoom,
            _ => 1.0,
        };
        let axis = camera.rotation * DVec3::X;
        Self::new(
            DVec2::new(camera.position.x, camera.position.y),
            camera.frame_width * 0.5,
            camera.frame_height * 0.5,
            axis.y.atan2(axis.x),
            zoom,
        )
    }

    fn corners(&self) -> [DVec2; 4] {
        let (w, h) = (self.half_width, self.half_height);
        [
            DVec2::new(-w, -h),
            DVec2::new(w, -h),
            DVec2::new(w, h),
            DVec2::new(-w, h),
        ]
    }

    /// Distance from the frame center to a corner.
    fn reach(&self) -> f64 {
        self.half_width.hypot(self.half_height)
    }

    fn world_path(&self, mut path: BezPath) -> BezPath {
        path.apply_affine(self.view);
        path
    }

    fn world_point(&self, point: DVec2) -> Point {
        self.view * Point::new(point.x, point.y)
    }

    fn world_vector(&self, vector: DVec2) -> DVec2 {
        let a = self.world_point(vector);
        let o = self.world_point(DVec2::ZERO);
        DVec2::new(a.x - o.x, a.y - o.y)
    }

    fn frame_point(&self, world: DVec2) -> DVec2 {
        let p = self.view.inverse() * Point::new(world.x, world.y);
        DVec2::new(p.x, p.y)
    }

    fn frame_rect(&self, offset: DVec2) -> BezPath {
        Rect::new(
            -self.half_width + offset.x,
            -self.half_height + offset.y,
            self.half_width + offset.x,
            self.half_height + offset.y,
        )
        .to_path(0.1)
    }

    /// A rectangle far larger than the frame, used to build complements.
    fn outer_rect(&self) -> BezPath {
        let r = self.reach() * 12.0;
        Rect::new(-r, -r, r, r).to_path(0.1)
    }

    /// Complement of `inside` (frame space) as an even-odd world mask.
    fn complement(&self, inside: &BezPath) -> TransitionMask {
        let mut path = self.outer_rect();
        path.extend(inside.iter());
        TransitionMask {
            path: self.world_path(path),
            rule: Fill::EvenOdd,
            fade: None,
        }
    }

    fn hard(&self, path: BezPath) -> TransitionMask {
        TransitionMask::hard(self.world_path(path))
    }
}

/// Vector state of one side-splitting transition at a given progress.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionMasks {
    /// Visible region of the incoming scene (world space).
    pub incoming: Option<TransitionMask>,
    /// Visible region of the outgoing scene (world space).
    pub outgoing: Option<TransitionMask>,
    /// World translation of the incoming scene's roots.
    pub incoming_offset: DVec2,
    /// World translation of the outgoing scene's roots.
    pub outgoing_offset: DVec2,
}

fn polygon(points: &[DVec2]) -> BezPath {
    let mut path = BezPath::new();
    for (index, point) in points.iter().enumerate() {
        let point = Point::new(point.x, point.y);
        if index == 0 {
            path.move_to(point);
        } else {
            path.line_to(point);
        }
    }
    if !points.is_empty() {
        path.close_path();
    }
    path
}

/// Band `q0 <= p·d <= q1` of the plane, bounded laterally by `±lateral`.
fn band(d: DVec2, q0: f64, q1: f64, lateral: f64) -> BezPath {
    let n = d.perp();
    polygon(&[
        d * q0 - n * lateral,
        d * q1 - n * lateral,
        d * q1 + n * lateral,
        d * q0 + n * lateral,
    ])
}

/// Filled pie slice from `from` sweeping by `sweep` radians (negative = clockwise).
fn sector(radius: f64, from: f64, sweep: f64) -> BezPath {
    if sweep.abs() <= 1e-9 {
        return BezPath::new();
    }
    let steps = ((sweep.abs() / TAU) * 128.0).ceil().max(2.0) as usize;
    let mut points = Vec::with_capacity(steps + 2);
    points.push(DVec2::ZERO);
    for step in 0..=steps {
        let angle = from + sweep * step as f64 / steps as f64;
        points.push(DVec2::new(angle.cos(), angle.sin()) * radius);
    }
    polygon(&points)
}

/// Unit outline of a preset iris shape (outer radius 1, centered at the origin).
fn iris_unit_outline(shape: &IrisShape) -> BezPath {
    match shape {
        IrisShape::Circle | IrisShape::Drawable(_) => Circle::new((0.0, 0.0), 1.0).to_path(1e-4),
        IrisShape::Diamond => polygon(&[DVec2::X, DVec2::Y, DVec2::NEG_X, DVec2::NEG_Y]),
        IrisShape::Square => polygon(&[
            DVec2::new(1.0, 1.0),
            DVec2::new(-1.0, 1.0),
            DVec2::new(-1.0, -1.0),
            DVec2::new(1.0, -1.0),
        ]),
        IrisShape::Star => {
            let points: Vec<DVec2> = (0..10)
                .map(|index| {
                    let radius = if index % 2 == 0 { 1.0 } else { 0.45 };
                    let angle = PI / 2.0 + index as f64 * PI / 5.0;
                    DVec2::new(angle.cos(), angle.sin()) * radius
                })
                .collect();
            polygon(&points)
        }
    }
}

/// Center an arbitrary outline on its bounding box and scale it so its
/// largest half extent is 1.
pub fn normalize_outline(path: &BezPath) -> Option<BezPath> {
    let bounds = path.bounding_box();
    let half = (bounds.width().max(bounds.height())) * 0.5;
    if !half.is_finite() || half <= 1e-12 {
        return None;
    }
    let center = bounds.center();
    let mut normalized = path.clone();
    normalized.apply_affine(Affine::scale(1.0 / half) * Affine::translate(-center.to_vec2()));
    Some(normalized)
}

/// Smallest distance from the origin to the outline: once scaled by
/// `distance / inradius`, the shape contains every point within `distance`.
fn inradius(path: &BezPath) -> f64 {
    let mut min = f64::INFINITY;
    let mut start = None;
    let mut last = None;
    let origin = Point::ORIGIN;
    let mut visit = |a: Point, b: Point| {
        let distance = Line::new(a, b).nearest(origin, 1e-6).distance_sq.sqrt();
        min = min.min(distance);
    };
    gaanim_core::kurbo::flatten(path.iter(), 1e-3, |element| match element {
        PathEl::MoveTo(point) => {
            if let (Some(first), Some(previous)) = (start, last) {
                visit(previous, first);
            }
            start = Some(point);
            last = Some(point);
        }
        PathEl::LineTo(point) => {
            if let Some(previous) = last {
                visit(previous, point);
            }
            last = Some(point);
        }
        PathEl::ClosePath => {
            if let (Some(first), Some(previous)) = (start, last) {
                visit(previous, first);
            }
            last = start;
        }
        _ => {}
    });
    if let (Some(first), Some(previous)) = (start, last) {
        visit(previous, first);
    }
    if min.is_finite() { min.max(0.05) } else { 1.0 }
}

/// Masks and offsets of a vector transition at eased progress `t`.
///
/// Returns `None` for transitions that are not vector reveals. `iris_outline`
/// supplies the (any-space) outline of an `IrisShape::Drawable`.
pub fn transition_masks(
    transition: &TransitionType,
    t: f64,
    view: &TransitionView,
    iris_outline: Option<&BezPath>,
) -> Option<TransitionMasks> {
    let t = if t.is_finite() { t } else { 1.0 };
    let clamped = t.clamp(0.0, 1.0);
    let mut masks = TransitionMasks {
        incoming: None,
        outgoing: None,
        incoming_offset: DVec2::ZERO,
        outgoing_offset: DVec2::ZERO,
    };
    match transition.base() {
        TransitionType::Slide { direction, .. } | TransitionType::Push { direction, .. } => {
            let push = matches!(transition.base(), TransitionType::Push { .. });
            let d = direction.vector();
            let span = match direction {
                SlideDirection::Left | SlideDirection::Right => 2.0 * view.half_width,
                SlideDirection::Up | SlideDirection::Down => 2.0 * view.half_height,
            };
            let incoming_offset = -d * span * (1.0 - t);
            let incoming_rect = view.frame_rect(incoming_offset);
            masks.incoming_offset = view.world_vector(incoming_offset);
            masks.incoming = Some(view.hard(incoming_rect.clone()));
            if push {
                let outgoing_offset = d * span * t;
                masks.outgoing_offset = view.world_vector(outgoing_offset);
                masks.outgoing = Some(view.hard(view.frame_rect(outgoing_offset)));
            } else {
                masks.outgoing = Some(view.complement(&incoming_rect));
            }
        }
        TransitionType::Wipe {
            direction, feather, ..
        } => {
            let d = direction.try_normalize().unwrap_or(DVec2::NEG_X);
            let (q_min, q_max) = view
                .corners()
                .iter()
                .map(|corner| corner.dot(d))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                    (lo.min(q), hi.max(q))
                });
            let length = (q_max - q_min).max(1e-9);
            let width = feather.clamp(0.0, 1.0) * length;
            let edge = q_min - width + (length + width) * clamped;
            let far = view.reach() * 4.0;
            let fade = |from: f64, to: f64| {
                (width > 1e-12).then(|| (view.world_point(d * from), view.world_point(d * to)))
            };
            masks.incoming = Some(TransitionMask {
                fade: fade(edge, edge + width),
                ..view.hard(band(d, -far, edge + width, far))
            });
            masks.outgoing = Some(TransitionMask {
                fade: fade(edge + width, edge),
                ..view.hard(band(d, edge, far, far))
            });
        }
        TransitionType::ClockWipe { start_angle, .. } => {
            let radius = view.reach() * 1.5;
            let from = start_angle.to_radians();
            let sweep = TAU * clamped;
            masks.incoming = Some(view.hard(sector(radius, from, -sweep)));
            masks.outgoing = Some(view.hard(sector(radius, from - sweep, -(TAU - sweep))));
        }
        TransitionType::Iris { center, shape, .. } => {
            let unit = match (shape, iris_outline) {
                (IrisShape::Drawable(_), Some(outline)) => normalize_outline(outline)
                    .unwrap_or_else(|| iris_unit_outline(&IrisShape::Circle)),
                _ => iris_unit_outline(shape),
            };
            let center = view.frame_point(*center);
            let cover = view
                .corners()
                .iter()
                .map(|corner| corner.distance(center))
                .fold(0.0, f64::max)
                / inradius(&unit);
            let scale = cover * t.max(0.0);
            let mut outline = unit;
            outline.apply_affine(Affine::translate((center.x, center.y)) * Affine::scale(scale));
            masks.outgoing = Some(view.complement(&outline));
            masks.incoming = Some(view.hard(outline));
        }
        TransitionType::Blinds { count, angle, .. } => {
            let count = (*count).max(1);
            let theta = angle.to_radians();
            let u = DVec2::new(theta.cos(), theta.sin());
            let n = u.perp();
            let (n_min, n_max) = view
                .corners()
                .iter()
                .map(|corner| corner.dot(n))
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), q| {
                    (lo.min(q), hi.max(q))
                });
            let slat = (n_max - n_min) / count as f64;
            let lateral = view.reach() * 2.0;
            let strip = |top: f64, bottom: f64| {
                polygon(&[
                    n * top - u * lateral,
                    n * top + u * lateral,
                    n * bottom + u * lateral,
                    n * bottom - u * lateral,
                ])
            };
            let mut incoming = BezPath::new();
            let mut outgoing = BezPath::new();
            for index in 0..count {
                let top = n_max - index as f64 * slat;
                let open = top - slat * clamped;
                if clamped > 0.0 {
                    incoming.extend(strip(top, open).iter());
                }
                if clamped < 1.0 {
                    outgoing.extend(strip(open, top - slat).iter());
                }
            }
            masks.incoming = Some(view.hard(incoming));
            masks.outgoing = Some(view.hard(outgoing));
        }
        _ => return None,
    }
    Some(masks)
}

/// Smooth 0 → 1 → 0 bump over `tau` in `[0, 1]`, peaking at 0.5.
fn bump(tau: f64) -> f64 {
    let x = (1.0 - (2.0 * tau - 1.0).abs()).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

fn hsv(hue: f64, saturation: f64, value: f64, alpha: f64) -> Color {
    let h = hue.rem_euclid(1.0) * 6.0;
    let c = value * saturation;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = value - c;
    Color::new([
        (r + m) as f32,
        (g + m) as f32,
        (b + m) as f32,
        alpha.clamp(0.0, 1.0) as f32,
    ])
}

/// Overlay layers at overlay progress `tau` in `[0, 1]`.
pub fn overlay_layers(
    overlay: &TransitionOverlay,
    tau: f64,
    view: &TransitionView,
) -> Vec<TransitionOverlayLayer> {
    let tau = tau.clamp(0.0, 1.0);
    let clip_path = view.world_path(
        Rect::new(
            -view.half_width,
            -view.half_height,
            view.half_width,
            view.half_height,
        )
        .inflate(view.half_width * 0.02, view.half_height * 0.02)
        .to_path(0.1),
    );
    match overlay {
        TransitionOverlay::Flash { color, .. } => {
            let alpha = bump(tau) as f32;
            if alpha <= 0.0 {
                return Vec::new();
            }
            vec![TransitionOverlayLayer {
                blend: BlendMode::default(),
                clip: clip_path.clone(),
                fills: vec![(clip_path, Brush::Solid(color.multiply_alpha(alpha)))],
            }]
        }
        TransitionOverlay::LightLeak {
            seed,
            hue,
            intensity,
            ..
        } => {
            let envelope = bump(tau) * intensity.clamp(0.0, 4.0);
            if envelope <= 0.0 {
                return Vec::new();
            }
            let mut rng = gaanim_math::SeededRng::new(*seed);
            let (w, h) = (view.half_width, view.half_height);
            let fills = (0..3)
                .map(|_| {
                    let base = DVec2::new(rng.uniform(-0.9, 0.9) * w, rng.uniform(-0.8, 0.8) * h);
                    let drift = DVec2::new(rng.uniform(-0.7, 0.7) * w, rng.uniform(-0.4, 0.4) * h);
                    let radius = rng.uniform(0.9, 1.6) * h;
                    let blob_hue = hue + rng.uniform(-0.06, 0.06);
                    let saturation = rng.uniform(0.55, 0.9);
                    let phase = rng.uniform(0.0, TAU);
                    let flicker = 0.75 + 0.25 * (tau * TAU + phase).sin();
                    let strength = (envelope * flicker).clamp(0.0, 1.0);
                    let center = view.world_point(base + drift * (tau - 0.5));
                    let core = hsv(blob_hue, saturation, 1.0, strength);
                    let gradient = Gradient::new_radial(center, (radius * view.scale) as f32)
                        .with_stops([
                            (0.0_f32, core),
                            (0.45_f32, core.multiply_alpha(0.45)),
                            (1.0_f32, core.with_alpha(0.0)),
                        ]);
                    (clip_path.clone(), Brush::Gradient(gradient))
                })
                .collect();
            vec![TransitionOverlayLayer {
                blend: BlendMode::new(Mix::Screen, Compose::SrcOver),
                clip: clip_path,
                fills,
            }]
        }
    }
}

/// Reset the published transition frame before a seek re-evaluates it.
pub(crate) fn reset_transition_frame(world: &mut World) {
    match world.get_resource_mut::<SceneTransitionFrame>() {
        Some(mut frame) => {
            if !frame.is_empty() || !frame.incoming.is_empty() || !frame.outgoing.is_empty() {
                *frame = SceneTransitionFrame::default();
            }
        }
        None => world.insert_resource(SceneTransitionFrame::default()),
    }
}

fn transition_view(world: &World) -> TransitionView {
    world
        .get_resource::<gaanim_math::Camera>()
        .map(TransitionView::from_camera)
        .unwrap_or_else(|| TransitionView::new(DVec2::ZERO, 8.0, 4.5, 0.0, 1.0))
}

/// Apply a vector reveal: offset scene roots and publish masks and memberships.
///
/// `t` is the eased progress. Returns `false` when `transition` is not a
/// vector reveal.
pub(crate) fn apply_vector_transition(
    world: &mut World,
    scene_entities: &[(Entity, SceneId)],
    transition: &TransitionType,
    t: f64,
    from: SceneId,
    to: SceneId,
) -> bool {
    let view = transition_view(world);
    let outline = match transition.base() {
        TransitionType::Iris {
            shape: IrisShape::Drawable(id),
            ..
        } => {
            let mut query = world.query::<(&MobjectId, &Path2D)>();
            query
                .iter(world)
                .find(|(mobject, _)| mobject.0 == *id)
                .map(|(_, path)| (*path.0).clone())
        }
        _ => None,
    };
    let Some(masks) = transition_masks(transition, t, &view, outline.as_ref()) else {
        return false;
    };

    let mut frame = SceneTransitionFrame::default();
    for (entity, scene) in scene_entities {
        let offset = if *scene == from {
            frame.outgoing.insert(*entity);
            masks.outgoing_offset
        } else if *scene == to {
            frame.incoming.insert(*entity);
            masks.incoming_offset
        } else {
            continue;
        };
        // Children inherit their root's translation.
        if offset == DVec2::ZERO || world.get::<ChildOf>(*entity).is_some() {
            continue;
        }
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(*entity) {
            transform.translation.x += offset.x;
            transform.translation.y += offset.y;
        }
    }
    frame.incoming_mask = masks.incoming;
    frame.outgoing_mask = masks.outgoing;
    world.insert_resource(frame);
    true
}

/// Record the segment-background times and overlay layers of this seek.
pub(crate) fn finish_transition_frame(
    world: &mut World,
    span: Option<(f64, f64)>,
    overlays: &[(TransitionOverlay, f64)],
) {
    let view = (!overlays.is_empty()).then(|| transition_view(world));
    let Some(mut frame) = world.get_resource_mut::<SceneTransitionFrame>() else {
        return;
    };
    if frame.incoming_mask.is_some() || frame.outgoing_mask.is_some() {
        frame.backgrounds = span.map(|(start, end)| (start - 1e-4, end));
    }
    if let Some(view) = view {
        for (overlay, tau) in overlays {
            frame.overlays.extend(overlay_layers(overlay, *tau, &view));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> TransitionView {
        TransitionView::new(DVec2::ZERO, 8.0, 4.5, 0.0, 1.0)
    }

    fn inside(mask: &TransitionMask, x: f64, y: f64) -> bool {
        let winding = mask.path.winding(Point::new(x, y));
        match mask.rule {
            Fill::NonZero => winding != 0,
            Fill::EvenOdd => winding % 2 != 0,
        }
    }

    fn masks(transition: TransitionType, t: f64) -> TransitionMasks {
        transition_masks(&transition, t, &view(), None).expect("vector transition")
    }

    #[test]
    fn hard_wipe_splits_the_frame_at_its_midpoint() {
        let m = masks(TransitionType::wipe(1.0, DVec2::NEG_X, 0.0), 0.5);
        let (incoming, outgoing) = (m.incoming.unwrap(), m.outgoing.unwrap());
        // Travelling left, the edge starts at the right edge of the frame.
        assert!(inside(&incoming, 4.0, 0.0) && !inside(&incoming, -4.0, 0.0));
        assert!(inside(&outgoing, -4.0, 0.0) && !inside(&outgoing, 4.0, 0.0));
        assert!(incoming.fade.is_none());
    }

    #[test]
    fn feathered_wipe_starts_hidden_and_ends_revealed() {
        let start = masks(TransitionType::wipe(1.0, DVec2::X, 0.2), 0.0);
        let incoming = start.incoming.unwrap();
        assert!(!inside(&incoming, -7.9, 0.0));
        let (opaque, clear) = incoming.fade.expect("feather ramp");
        assert!(clear.x <= -8.0 + 1e-9 && opaque.x < clear.x);

        let end = masks(TransitionType::wipe(1.0, DVec2::X, 0.2), 1.0);
        let (opaque, _) = end.incoming.unwrap().fade.unwrap();
        assert!(
            opaque.x >= 8.0 - 1e-9,
            "fully opaque over the frame at t = 1"
        );
    }

    #[test]
    fn clock_wipe_sweeps_clockwise_from_twelve() {
        let m = masks(TransitionType::clock_wipe(1.0, 90.0), 0.25);
        let (incoming, outgoing) = (m.incoming.unwrap(), m.outgoing.unwrap());
        assert!(inside(&incoming, 2.0, 2.0));
        assert!(!inside(&incoming, -2.0, 2.0) && inside(&outgoing, -2.0, 2.0));
        assert!(!inside(&incoming, 2.0, -2.0));
    }

    #[test]
    fn iris_shapes_grow_from_their_center_until_the_frame_is_covered() {
        for shape in [
            IrisShape::Circle,
            IrisShape::Diamond,
            IrisShape::Square,
            IrisShape::Star,
        ] {
            let center = DVec2::new(2.0, 1.0);
            let half = masks(TransitionType::iris(1.0, center, shape.clone()), 0.5);
            assert!(
                inside(half.incoming.as_ref().unwrap(), 2.0, 1.0),
                "{shape:?}"
            );
            assert!(
                !inside(half.outgoing.as_ref().unwrap(), 2.0, 1.0),
                "{shape:?}"
            );

            let full = masks(TransitionType::iris(1.0, center, shape.clone()), 1.0);
            let incoming = full.incoming.unwrap();
            for (x, y) in [(-8.0, -4.5), (8.0, -4.5), (8.0, 4.5), (-8.0, 4.5)] {
                assert!(inside(&incoming, x * 0.999, y * 0.999), "{shape:?} corner");
            }
        }
    }

    #[test]
    fn iris_accepts_an_arbitrary_outline() {
        let outline = Rect::new(10.0, 10.0, 14.0, 12.0).to_path(0.1);
        let transition = TransitionType::iris(
            1.0,
            DVec2::ZERO,
            IrisShape::Drawable(gaanim_core::ObjectId::from_raw(7)),
        );
        let m = transition_masks(&transition, 1.0, &view(), Some(&outline)).unwrap();
        assert!(inside(m.incoming.as_ref().unwrap(), 7.9, 4.4));
    }

    #[test]
    fn blinds_open_every_slat_by_the_same_fraction() {
        let m = masks(TransitionType::blinds(1.0, 4, 0.0), 0.5);
        let (incoming, outgoing) = (m.incoming.unwrap(), m.outgoing.unwrap());
        // Slats are 2.25 tall; the top half of each is open.
        for top in [4.5, 2.25, 0.0, -2.25] {
            assert!(inside(&incoming, 0.0, top - 0.5));
            assert!(inside(&outgoing, 0.0, top - 1.75));
            assert!(!inside(&incoming, 0.0, top - 1.75));
        }
    }

    #[test]
    fn push_moves_both_scenes_while_slide_covers_the_outgoing_one() {
        let push = masks(TransitionType::push(1.0, SlideDirection::Left), 0.5);
        assert_eq!(push.incoming_offset, DVec2::new(8.0, 0.0));
        assert_eq!(push.outgoing_offset, DVec2::new(-8.0, 0.0));

        let slide = masks(TransitionType::slide(1.0, SlideDirection::Up), 0.25);
        assert_eq!(slide.outgoing_offset, DVec2::ZERO);
        assert!((slide.incoming_offset.y + 6.75).abs() < 1e-9);
        let outgoing = slide.outgoing.unwrap();
        assert!(inside(&outgoing, 0.0, 4.0) && !inside(&outgoing, 0.0, -4.0));
    }

    #[test]
    fn camera_zoom_and_offset_map_frame_space_to_world() {
        let zoomed = TransitionView::new(DVec2::new(10.0, 0.0), 8.0, 4.5, 0.0, 2.0);
        let transition = TransitionType::push(1.0, SlideDirection::Right);
        let m = transition_masks(&transition, 0.5, &zoomed, None).unwrap();
        assert_eq!(m.outgoing_offset, DVec2::new(4.0, 0.0));
        assert!(inside(m.outgoing.as_ref().unwrap(), 13.0, 0.0));
    }

    #[test]
    fn easing_defaults_and_overrides() {
        let wipe = TransitionType::wipe(1.0, DVec2::X, 0.0);
        assert!(
            (wipe.eased_progress(0.25) - gaanim_math::RateFunc::Smooth.evaluate(0.25)).abs()
                < 1e-12
        );
        assert_eq!(TransitionType::cross_fade(1.0).eased_progress(0.25), 0.25);
        let eased = TransitionType::cross_fade(1.0).with_easing(gaanim_math::RateFunc::Smooth);
        assert!((eased.eased_progress(0.25) - 0.15625).abs() < 1e-12);
        assert!(matches!(eased.base(), TransitionType::CrossFade { .. }));
        assert_eq!(eased.duration(), 1.0);
        let decorated = eased.with_overlay(TransitionOverlay::flash(Color::WHITE, 0.2));
        assert!(decorated.overlay().is_some() && decorated.eased_progress(0.25) != 0.25);
    }

    #[test]
    fn seeks_publish_the_transition_frame_only_while_it_is_active() {
        use crate::clip::ClipPayload;
        use crate::scene::SceneMember;
        use crate::timeline::Timeline;
        use gaanim_core::ObjectId;

        let mut world = World::new();
        let mut timeline = Timeline::default();
        let track = timeline.add_track("main", 0);
        let first = timeline.add_scene("first");
        let second = timeline.add_scene("second");
        timeline.index_scene(first, 0.0);
        timeline.index_scene(second, 1.0);
        timeline.add_clip(track, 0.0, 0.0, ClipPayload::SceneStart(first));
        timeline.add_clip(track, 1.0, 0.0, ClipPayload::SceneEnd(first));
        timeline.add_clip(track, 1.0, 0.0, ClipPayload::SceneStart(second));
        timeline.add_clip(track, 3.0, 0.0, ClipPayload::SceneEnd(second));
        timeline.connect(
            first,
            second,
            TransitionType::push(1.0, SlideDirection::Left)
                .with_overlay(TransitionOverlay::flash(Color::WHITE, 0.4)),
        );
        let mut spawn = |raw, scene| {
            world
                .spawn((
                    MobjectId(ObjectId::from_raw(raw)),
                    SpatialTransform::default(),
                    gaanim_scene::Opacity(1.0),
                    SceneMember(scene),
                ))
                .id()
        };
        let outgoing = spawn(1, first);
        let incoming = spawn(2, second);

        timeline.seek(&mut world, 1.5);
        let frame = world.resource::<SceneTransitionFrame>();
        assert!(frame.incoming.contains(&incoming) && frame.outgoing.contains(&outgoing));
        assert!(frame.incoming_mask.is_some() && frame.outgoing_mask.is_some());
        assert_eq!(frame.backgrounds, Some((1.0 - 1e-4, 2.0)));
        assert_eq!(frame.overlays.len(), 1, "the flash spans 1.5 ± 0.2");
        // Eased progress 0.5 of a 16-unit frame (fallback view without a camera).
        let x = |entity| world.get::<SpatialTransform>(entity).unwrap().translation.x;
        assert_eq!((x(incoming), x(outgoing)), (8.0, -8.0));

        timeline.seek(&mut world, 1.9);
        let frame = world.resource::<SceneTransitionFrame>();
        assert!(frame.overlays.is_empty() && frame.incoming_mask.is_some());

        timeline.seek(&mut world, 2.5);
        assert!(world.resource::<SceneTransitionFrame>().is_empty());
    }

    #[test]
    fn overlays_are_centered_on_the_cut_and_deterministic() {
        let flash = TransitionOverlay::flash(Color::WHITE, 0.2);
        let (start, end) = flash.window(3.0, 0.0);
        assert!((start - 2.9).abs() < 1e-12 && (end - 3.1).abs() < 1e-12);
        assert!(overlay_layers(&flash, 0.0, &view()).is_empty());
        let peak = overlay_layers(&flash, 0.5, &view());
        let Brush::Solid(color) = &peak[0].fills[0].1 else {
            panic!("flash is a solid fill")
        };
        assert!((color.components[3] - 1.0).abs() < 1e-6);

        let leak = TransitionOverlay::light_leak(2, 0.1, 0.8, 1.0);
        let a = format!("{:?}", overlay_layers(&leak, 0.4, &view()));
        let b = format!("{:?}", overlay_layers(&leak, 0.4, &view()));
        let other = format!(
            "{:?}",
            overlay_layers(
                &TransitionOverlay::light_leak(3, 0.1, 0.8, 1.0),
                0.4,
                &view()
            )
        );
        assert_eq!(a, b);
        assert_ne!(a, other);
    }
}
