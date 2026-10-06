//! Axonometric drawings: flat polygons and polylines whose vertices are
//! points of a 3D model projected by an orthographic view.

use std::sync::{Arc, Mutex};

use gaanim_core::glam::{DVec2, DVec3};

use super::canvas_impl::{Composition, spawn_in};
use super::ops::SharedCanvasState;
use super::types::SpawnKind;
use super::{DrawableHandle, SceneModel};

/// An orthographic view of a model with `z` up and its plan on `x`/`y`.
///
/// The view turns `azimuth` radians about `z` from the front view (looking
/// along `+y`, with `+x` to the right) and rises `elevation` radians above
/// the horizon. A model unit along the least shortened axis measures
/// `scale` scene units, and the model origin lands at `origin`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AxonometricView {
    pub azimuth: f64,
    pub elevation: f64,
    pub origin: DVec2,
    pub scale: f64,
}

impl AxonometricView {
    pub fn new(azimuth: f64, elevation: f64, origin: DVec2, scale: f64) -> Result<Self, String> {
        if !(azimuth.is_finite() && elevation.is_finite() && origin.is_finite()) {
            return Err("azimuth, elevation and origin must be finite".to_string());
        }
        if !(scale.is_finite() && scale > 0.0) {
            return Err("scale must be a positive number".to_string());
        }
        Ok(Self {
            azimuth,
            elevation,
            origin,
            scale,
        })
    }

    /// Azimuth and elevation of a named view: `isometric`, `dimetric` (the
    /// 2:1 drawing, whose receding axes rise one unit every two), `plan`,
    /// `front` or `side` (from `+x`).
    pub fn preset(name: &str) -> Option<(f64, f64)> {
        use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6};
        Some(match name {
            "isometric" => (FRAC_PI_4, std::f64::consts::FRAC_1_SQRT_2.atan()),
            "dimetric" => (FRAC_PI_4, FRAC_PI_6),
            "plan" => (0.0, FRAC_PI_2),
            "front" => (0.0, 0.0),
            "side" => (FRAC_PI_2, 0.0),
            _ => return None,
        })
    }

    /// The screen's right and up directions in the model, and the direction
    /// toward the viewer.
    fn basis(&self) -> (DVec3, DVec3, DVec3) {
        // A right angle's cosine comes out as 6e-17, which would leave the
        // plan and elevations a hair off the model's coordinates.
        let exact = |(sin, cos): (f64, f64)| {
            let snap = |value: f64| if value.abs() < 1e-12 { 0.0 } else { value };
            (snap(sin), snap(cos))
        };
        let (sin_a, cos_a) = exact(self.azimuth.sin_cos());
        let (sin_e, cos_e) = exact(self.elevation.sin_cos());
        (
            DVec3::new(cos_a, sin_a, 0.0),
            DVec3::new(-sin_a * sin_e, cos_a * sin_e, cos_e),
            DVec3::new(sin_a * cos_e, -cos_a * cos_e, sin_e),
        )
    }

    /// Scene units per model unit along the least shortened axis.
    fn unit(&self) -> f64 {
        let (right, up, _) = self.basis();
        let longest = [DVec3::X, DVec3::Y, DVec3::Z]
            .into_iter()
            .map(|axis| DVec2::new(right.dot(axis), up.dot(axis)).length())
            .fold(0.0, f64::max);
        self.scale / longest
    }

    /// Where the model point `point` is drawn.
    pub fn point(&self, point: DVec3) -> DVec2 {
        let (right, up, _) = self.basis();
        self.origin + self.unit() * DVec2::new(right.dot(point), up.dot(point))
    }

    /// How near the viewer the model point `point` is: larger is nearer.
    pub fn depth(&self, point: DVec3) -> f64 {
        self.basis().2.dot(point)
    }

    fn points(&self, points: &[DVec3]) -> Vec<(f64, f64)> {
        points
            .iter()
            .map(|point| {
                let point = self.point(*point);
                (point.x, point.y)
            })
            .collect()
    }
}

/// A drawing made by an [`Axonometric`].
#[derive(Debug, Clone)]
struct Drawing {
    drawable: DrawableHandle,
    points: Vec<DVec3>,
    /// A polygon or closed polyline, which covers what it encloses.
    closed: bool,
}

#[derive(Debug)]
struct Drawings {
    view: AxonometricView,
    drawings: Vec<Drawing>,
}

/// An axonometric view of a model and the drawings made with it: polygons
/// and polylines whose vertices are model points, drawn as flat shapes with
/// the scene's usual style. [`Self::animate_to`] moves them to another view.
#[derive(Debug, Clone)]
pub struct Axonometric {
    state: SharedCanvasState,
    inner: Arc<Mutex<Drawings>>,
}

impl SceneModel {
    /// A projection that draws model points with `view`.
    pub fn axonometric(&self, view: AxonometricView) -> Axonometric {
        Axonometric {
            state: self.state.clone(),
            inner: Arc::new(Mutex::new(Drawings {
                view,
                drawings: Vec::new(),
            })),
        }
    }
}

impl Axonometric {
    fn lock(&self) -> std::sync::MutexGuard<'_, Drawings> {
        self.inner.lock().expect("axonometric drawings poisoned")
    }

    /// The view this projection draws with now.
    pub fn view(&self) -> AxonometricView {
        self.lock().view
    }

    /// Where the model point `point` is drawn.
    pub fn point(&self, point: DVec3) -> DVec2 {
        self.view().point(point)
    }

    /// How near the viewer the model point `point` is: larger is nearer.
    pub fn depth(&self, point: DVec3) -> f64 {
        self.view().depth(point)
    }

    /// A polygon through the model points `points`.
    pub fn polygon(&self, points: &[DVec3]) -> Result<DrawableHandle, String> {
        if points.len() < 3 {
            return Err("an axonometric polygon needs at least 3 points".to_string());
        }
        self.draw(points, true, SpawnKind::Polygon)
    }

    /// A polyline through the model points `points`; `closed` joins the last
    /// to the first.
    pub fn polyline(&self, points: &[DVec3], closed: bool) -> Result<DrawableHandle, String> {
        if points.len() < 2 {
            return Err("an axonometric polyline needs at least 2 points".to_string());
        }
        self.draw(points, closed && points.len() > 2, |projected| {
            SpawnKind::Polyline {
                points: projected,
                closed,
            }
        })
    }

    /// The segment between the model points `start` and `end`, as a
    /// polyline of two points.
    pub fn line(&self, start: DVec3, end: DVec3) -> Result<DrawableHandle, String> {
        self.polyline(&[start, end], false)
    }

    fn draw(
        &self,
        points: &[DVec3],
        closed: bool,
        kind: impl FnOnce(Vec<(f64, f64)>) -> SpawnKind,
    ) -> Result<DrawableHandle, String> {
        if points.iter().any(|point| !point.is_finite()) {
            return Err("model points must be finite".to_string());
        }
        let mut drawings = self.lock();
        let drawable = spawn_in(&self.state, kind(drawings.view.points(points)), true);
        // A face seen edge-on encloses nothing: a centered stroke still draws
        // it as the line it is, where the default inside stroke would vanish.
        let drawable = drawable.stroke_align(gaanim_renderer::effects::StrokeAlign::Center);
        drawings.drawings.push(Drawing {
            drawable: drawable.clone(),
            points: points.to_vec(),
            closed,
        });
        Ok(drawable)
    }

    /// Give `drawables`, drawn with this projection, `z_index` values from
    /// `z_index` up, back to front in the current view, and return them in
    /// that order. A drawing that covers part of another and is nearer the
    /// viewer there draws above it; the rest keep their mean depth.
    pub fn depth_sort(
        &self,
        drawables: &[DrawableHandle],
        z_index: i32,
    ) -> Result<Vec<DrawableHandle>, String> {
        self.depth_sort_in(drawables, z_index, self.view())
    }

    /// [`Self::depth_sort`] as seen from `view`, such as the view the
    /// drawings will move to.
    pub fn depth_sort_in(
        &self,
        drawables: &[DrawableHandle],
        z_index: i32,
        view: AxonometricView,
    ) -> Result<Vec<DrawableHandle>, String> {
        let items = {
            let inner = self.lock();
            let mut items = Vec::with_capacity(drawables.len());
            for drawable in drawables {
                let drawing = inner
                    .drawings
                    .iter()
                    .find(|drawing| drawing.drawable.id == drawable.id)
                    .ok_or("depth_sort takes drawables drawn with this projection")?;
                if items
                    .iter()
                    .any(|item: &Drawing| item.drawable.id == drawable.id)
                {
                    return Err("depth_sort takes each drawable once".to_string());
                }
                items.push(drawing.clone());
            }
            items
        };
        if i32::try_from(items.len())
            .ok()
            .and_then(|count| z_index.checked_add(count))
            .is_none()
        {
            return Err("z_index leaves no room for every drawable".to_string());
        }
        let order = back_to_front(&view, &items);
        Ok(order
            .into_iter()
            .zip(z_index..)
            .map(|(index, z)| items[index].drawable.clone().z_index(z))
            .collect())
    }

    /// Move every drawing of this projection to where `other`'s view draws
    /// its model points, as one composition, and draw with that view from
    /// now on. Vertices travel straight, as with `animate.points`.
    pub fn animate_to(&self, other: &Axonometric) -> Result<Composition, String> {
        let view = other.view();
        let mut inner = self.lock();
        if inner.drawings.is_empty() {
            return Err("this projection has drawn nothing to animate".to_string());
        }
        let moves = inner
            .drawings
            .iter()
            .map(|drawing| {
                drawing
                    .drawable
                    .animate()
                    .points(view.points(&drawing.points))
                    .map(Composition::leaf)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let composition = Composition::parallel(moves).map_err(|error| error.to_string())?;
        inner.view = view;
        Ok(composition)
    }
}

/// A drawing seen in a view: its outline on screen, unscaled, and how near
/// the viewer each vertex is.
struct Seen {
    screen: Vec<DVec2>,
    depths: Vec<f64>,
    /// The outline joins its last point to its first.
    closed: bool,
    /// A face not seen edge-on, which covers what it encloses on screen.
    region: bool,
    /// Plane of a face not seen edge-on, as a normal and offset.
    plane: Option<(DVec3, f64)>,
    min: DVec2,
    max: DVec2,
    mean: f64,
}

const TOUCHING: f64 = 1e-9;

fn see(view: &AxonometricView, drawing: &Drawing) -> Seen {
    let (right, up, toward) = view.basis();
    let screen: Vec<DVec2> = drawing
        .points
        .iter()
        .map(|point| DVec2::new(right.dot(*point), up.dot(*point)))
        .collect();
    let depths: Vec<f64> = drawing
        .points
        .iter()
        .map(|point| toward.dot(*point))
        .collect();
    let plane = drawing
        .closed
        .then(|| newell_plane(&drawing.points))
        .flatten()
        .filter(|(normal, _)| normal.dot(toward).abs() > 1e-6 * normal.length());
    let min = screen.iter().copied().fold(DVec2::INFINITY, DVec2::min);
    let max = screen.iter().copied().fold(DVec2::NEG_INFINITY, DVec2::max);
    let mean = depths.iter().sum::<f64>() / depths.len() as f64;
    Seen {
        screen,
        depths,
        closed: drawing.closed,
        region: plane.is_some(),
        plane,
        min,
        max,
        mean,
    }
}

/// The plane through a face's points, by Newell's method.
fn newell_plane(points: &[DVec3]) -> Option<(DVec3, f64)> {
    let mut normal = DVec3::ZERO;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        normal.x += (point.y - next.y) * (point.z + next.z);
        normal.y += (point.z - next.z) * (point.x + next.x);
        normal.z += (point.x - next.x) * (point.y + next.y);
    }
    let length = normal.length();
    if length <= f64::EPSILON {
        return None;
    }
    let centroid = points.iter().copied().sum::<DVec3>() / points.len() as f64;
    let normal = normal / length;
    Some((normal, normal.dot(centroid)))
}

impl Seen {
    fn edges(&self) -> impl Iterator<Item = (usize, DVec2, DVec2)> + '_ {
        let count = self.screen.len();
        let edges = if self.closed { count } else { count - 1 };
        (0..edges).map(move |index| (index, self.screen[index], self.screen[(index + 1) % count]))
    }

    fn contains(&self, point: DVec2) -> bool {
        if !self.region {
            return false;
        }
        let mut inside = false;
        for (_, a, b) in self.edges() {
            if (a.y > point.y) != (b.y > point.y)
                && point.x < a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x)
            {
                inside = !inside;
            }
        }
        inside
    }

    /// How near the viewer the drawing is where it is seen at `point`.
    fn depth_at(&self, point: DVec2, view: &AxonometricView) -> f64 {
        if let Some((normal, offset)) = self.plane {
            let (right, up, toward) = view.basis();
            let on_screen = point.x * right + point.y * up;
            return (offset - normal.dot(on_screen)) / normal.dot(toward);
        }
        // An outline: the depth of its nearest point on screen.
        let mut best = (f64::INFINITY, self.mean);
        let count = self.screen.len();
        for (index, a, b) in self.edges() {
            let along = b - a;
            let t = if along.length_squared() > 0.0 {
                ((point - a).dot(along) / along.length_squared()).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let distance = (a + t * along).distance_squared(point);
            if distance < best.0 {
                let (from, to) = (self.depths[index], self.depths[(index + 1) % count]);
                best = (distance, from + t * (to - from));
            }
        }
        if count == 1 {
            return self.depths[0];
        }
        best.1
    }
}

fn segment_intersection(a: DVec2, b: DVec2, c: DVec2, d: DVec2) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let denominator = r.perp_dot(s);
    if denominator.abs() <= f64::EPSILON {
        return None;
    }
    let t = (c - a).perp_dot(s) / denominator;
    let u = (c - a).perp_dot(r) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + t * r)
}

/// How much nearer the viewer `a` is than `b` where they overlap on screen,
/// on average; `None` where they do not overlap.
fn nearer(a: &Seen, b: &Seen, view: &AxonometricView) -> Option<f64> {
    if a.max.x < b.min.x || b.max.x < a.min.x || a.max.y < b.min.y || b.max.y < a.min.y {
        return None;
    }
    let mut points: Vec<DVec2> = a
        .screen
        .iter()
        .copied()
        .filter(|point| b.contains(*point))
        .chain(b.screen.iter().copied().filter(|point| a.contains(*point)))
        .collect();
    for (_, p, q) in a.edges() {
        for (_, r, s) in b.edges() {
            points.extend(segment_intersection(p, q, r, s));
        }
    }
    if points.is_empty() {
        return None;
    }
    let difference = points
        .iter()
        .map(|point| a.depth_at(*point, view) - b.depth_at(*point, view))
        .sum::<f64>()
        / points.len() as f64;
    Some(difference)
}

/// Indices of `drawings` from the farthest back: two drawings that overlap
/// on screen draw the nearer one later, and the rest by mean depth.
fn back_to_front(view: &AxonometricView, drawings: &[Drawing]) -> Vec<usize> {
    let seen: Vec<Seen> = drawings.iter().map(|drawing| see(view, drawing)).collect();
    let count = seen.len();
    let mut later: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut pending = vec![0usize; count];
    for i in 0..count {
        for j in i + 1..count {
            let Some(difference) = nearer(&seen[i], &seen[j], view) else {
                continue;
            };
            let scale = 1.0 + seen[i].mean.abs().max(seen[j].mean.abs());
            if difference.abs() <= TOUCHING * scale {
                continue;
            }
            let (first, second) = if difference < 0.0 { (i, j) } else { (j, i) };
            later[first].push(second);
            pending[second] += 1;
        }
    }
    let farther = |a: &usize, b: &usize| seen[*a].mean.total_cmp(&seen[*b].mean).then(a.cmp(b));
    let mut order = Vec::with_capacity(count);
    let mut ready: Vec<usize> = (0..count).filter(|index| pending[*index] == 0).collect();
    let mut placed = vec![false; count];
    loop {
        while let Some(position) = ready
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| farther(a, b))
            .map(|(position, _)| position)
        {
            let index = ready.swap_remove(position);
            placed[index] = true;
            order.push(index);
            for next in &later[index] {
                pending[*next] -= 1;
                if pending[*next] == 0 {
                    ready.push(*next);
                }
            }
        }
        // A cycle of overlaps: break it at its farthest drawing.
        let Some(stuck) = (0..count).filter(|index| !placed[*index]).min_by(farther) else {
            break;
        };
        pending[stuck] = 0;
        ready.push(stuck);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::FRAC_PI_2;

    fn view(name: &str) -> AxonometricView {
        let (azimuth, elevation) = AxonometricView::preset(name).unwrap();
        AxonometricView::new(azimuth, elevation, DVec2::ZERO, 1.0).unwrap()
    }

    fn near(a: DVec2, b: DVec2) -> bool {
        a.distance(b) < 1e-12
    }

    #[test]
    fn views_draw_the_model_axes_where_drawings_put_them() {
        let iso = view("isometric");
        let c = (std::f64::consts::PI / 6.0).cos();
        // Every axis keeps its length: x goes down to the right, y up to
        // the right and z straight up.
        assert!(near(iso.point(DVec3::X), DVec2::new(c, -0.5)));
        assert!(near(iso.point(DVec3::Y), DVec2::new(c, 0.5)));
        assert!(near(iso.point(DVec3::Z), DVec2::new(0.0, 1.0)));
        assert!(near(
            view("plan").point(DVec3::new(2.0, 3.0, 9.0)),
            DVec2::new(2.0, 3.0)
        ));
        assert!(near(
            view("front").point(DVec3::new(2.0, 9.0, 3.0)),
            DVec2::new(2.0, 3.0)
        ));
        assert!(near(
            view("side").point(DVec3::new(9.0, 2.0, 3.0)),
            DVec2::new(2.0, 3.0)
        ));
        let dimetric = view("dimetric").point(DVec3::Y);
        assert!((dimetric.y / dimetric.x - 0.5).abs() < 1e-12);

        let moved = AxonometricView::new(0.0, FRAC_PI_2, DVec2::new(1.0, -1.0), 0.5).unwrap();
        assert!(near(
            moved.point(DVec3::new(2.0, 2.0, 0.0)),
            DVec2::new(2.0, 0.0)
        ));
        assert!(view("plan").depth(DVec3::Z) > view("plan").depth(DVec3::ZERO));
        assert!(AxonometricView::new(0.0, 0.0, DVec2::ZERO, 0.0).is_err());
        assert!(AxonometricView::new(f64::NAN, 0.0, DVec2::ZERO, 1.0).is_err());
        assert!(AxonometricView::preset("perspective").is_none());
    }

    #[test]
    fn drawings_sort_by_depth_and_move_to_another_view() {
        use bevy::ecs::world::CommandQueue;
        use bevy::prelude::{Commands, World};

        let mut canvas = SceneModel::new(640, 360);
        let iso = canvas.axonometric(view("isometric"));
        let plan = canvas.axonometric(view("plan"));
        let square = |z: f64| {
            [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)].map(|(x, y)| DVec3::new(x, y, z))
        };
        let slab = iso.polygon(&square(1.0)).unwrap();
        // A tall column at the far corner: nearer than the slab on average,
        // but behind it where the slab covers it on screen.
        let column = iso
            .line(DVec3::new(0.0, 4.0, 0.0), DVec3::new(0.0, 4.0, 12.0))
            .unwrap();
        let floor = iso.polygon(&square(0.0)).unwrap();
        assert!(iso.polygon(&[DVec3::ZERO; 2]).is_err());
        assert!(iso.line(DVec3::NAN, DVec3::ZERO).is_err());
        assert!(iso.depth_sort(&[canvas.circle(1.0)], 0).is_err());
        assert!(iso.depth_sort(&[floor.clone(), floor.clone()], 0).is_err());
        assert!(iso.depth_sort(&[floor.clone()], i32::MAX).is_err());
        assert!(plan.animate_to(&iso).is_err(), "plan has drawn nothing");

        let order = iso
            .depth_sort(&[slab.clone(), column.clone(), floor.clone()], 10)
            .unwrap();
        assert_eq!(
            order.iter().map(|drawable| drawable.id).collect::<Vec<_>>(),
            [floor.id, column.id, slab.id]
        );
        let z_index = |drawable: &DrawableHandle| drawable.spec.lock().unwrap().z_index;
        assert_eq!([&floor, &column, &slab].map(z_index), [10, 11, 12]);
        // Drawn in plan, they sort as the isometric view they will move to.
        let in_plan =
            [square(1.0).to_vec(), square(0.0).to_vec()].map(|face| plan.polygon(&face).unwrap());
        let column_in_plan = plan
            .line(DVec3::new(0.0, 4.0, 0.0), DVec3::new(0.0, 4.0, 12.0))
            .unwrap();
        let sorted = plan
            .depth_sort_in(
                &[
                    in_plan[0].clone(),
                    column_in_plan.clone(),
                    in_plan[1].clone(),
                ],
                0,
                iso.view(),
            )
            .unwrap();
        assert_eq!(
            sorted
                .iter()
                .map(|drawable| drawable.id)
                .collect::<Vec<_>>(),
            [in_plan[1].id, column_in_plan.id, in_plan[0].id]
        );

        canvas
            .play_composition_configured(iso.animate_to(&plan).unwrap(), Some(1.0), None)
            .unwrap();
        assert_eq!(iso.view(), plan.view());
        assert_eq!(iso.point(DVec3::new(1.0, 2.0, 9.0)), DVec2::new(1.0, 2.0));

        let mut world = World::new();
        let mut queue = CommandQueue::default();
        let mut commands = Commands::new(&mut queue, &world);
        let mut timeline = gaanim_timeline::timeline::Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let config = gaanim_text::prelude::TextConfig::default();
        canvas.compile_into(&mut commands, &mut timeline, &fonts, &config);
        queue.apply(&mut world);
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        timeline.seek(&mut world, 1.0);
        let id = gaanim_core::ObjectId::from_raw(slab.id.as_raw() - 1);
        let path = world
            .query::<(&gaanim_scene::MobjectId, &gaanim_scene::Path2D)>()
            .iter(&world)
            .find(|(object, _)| object.0 == id)
            .unwrap()
            .1
            .0
            .clone();
        let corners: Vec<(f64, f64)> = path
            .elements()
            .iter()
            .filter_map(|element| match element {
                gaanim_core::kurbo::PathEl::MoveTo(point)
                | gaanim_core::kurbo::PathEl::LineTo(point) => Some((point.x, point.y)),
                _ => None,
            })
            .collect();
        assert_eq!(corners, [(0.0, 0.0), (4.0, 0.0), (4.0, 4.0), (0.0, 4.0)]);
    }

    fn drawing(points: &[(f64, f64, f64)], closed: bool) -> Drawing {
        let mut canvas = SceneModel::new(640, 360);
        Drawing {
            drawable: canvas.circle(1.0),
            points: points
                .iter()
                .map(|&(x, y, z)| DVec3::new(x, y, z))
                .collect(),
            closed,
        }
    }

    #[test]
    fn overlapping_drawings_draw_back_to_front() {
        let iso = view("isometric");
        // A slab above a floor, a wall in front of both and a column at the
        // far corner, listed front first.
        let wall = drawing(
            &[
                (0.0, 0.0, 0.0),
                (4.0, 0.0, 0.0),
                (4.0, 0.0, 3.0),
                (0.0, 0.0, 3.0),
            ],
            true,
        );
        let slab = drawing(
            &[
                (0.0, 0.0, 3.0),
                (4.0, 0.0, 3.0),
                (4.0, 4.0, 3.0),
                (0.0, 4.0, 3.0),
            ],
            true,
        );
        let floor = drawing(
            &[
                (0.0, 0.0, 0.0),
                (4.0, 0.0, 0.0),
                (4.0, 4.0, 0.0),
                (0.0, 4.0, 0.0),
            ],
            true,
        );
        let column = drawing(&[(0.0, 4.0, 0.0), (0.0, 4.0, 3.0)], false);
        let order = back_to_front(&iso, &[wall, slab, floor, column]);
        let rank = |index| order.iter().position(|&i| i == index).unwrap();
        assert!(rank(2) < rank(0), "the floor is behind the wall: {order:?}");
        assert!(rank(2) < rank(1), "the floor is under the slab: {order:?}");
        assert!(
            rank(3) < rank(0),
            "the far column is behind the wall: {order:?}"
        );
        // Seen from below the slab hides the floor's other side instead.
        let below =
            AxonometricView::new(std::f64::consts::FRAC_PI_4, -0.6, DVec2::ZERO, 1.0).unwrap();
        let floor = drawing(
            &[
                (0.0, 0.0, 0.0),
                (4.0, 0.0, 0.0),
                (4.0, 4.0, 0.0),
                (0.0, 4.0, 0.0),
            ],
            true,
        );
        let slab = drawing(
            &[
                (0.0, 0.0, 3.0),
                (4.0, 0.0, 3.0),
                (4.0, 4.0, 3.0),
                (0.0, 4.0, 3.0),
            ],
            true,
        );
        assert_eq!(back_to_front(&below, &[floor, slab]), [1, 0]);
    }
}
