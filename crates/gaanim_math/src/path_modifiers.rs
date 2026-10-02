//! Path modifiers in the manner of After Effects' shape operators: zig zag,
//! round corners, pucker & bloat, twist and wiggle. Each is a pure function
//! of a path and its parameters, so a modified path can be rebuilt at any
//! time; `gaanim_objects` adds offset, which needs boolean geometry.

use kurbo::{
    BezPath, CubicBez, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, PathSeg, Point, Vec2,
};

use crate::Noise;

/// Accuracy of arc-length measurements, in scene units.
const ARCLEN_ACCURACY: f64 = 1e-4;
/// Magic constant of a quarter circle drawn with one cubic.
const KAPPA: f64 = 0.552_284_749_830_793_4;

/// A sub-path as cubic segments; lines and quadratics are raised to cubics.
#[derive(Debug, Clone)]
struct Contour {
    segments: Vec<CubicBez>,
    closed: bool,
}

fn contours(path: &BezPath) -> Vec<Contour> {
    let mut contours = Vec::new();
    let mut segments: Vec<CubicBez> = Vec::new();
    let mut start = Point::ORIGIN;
    let mut current = Point::ORIGIN;
    let finish = |segments: &mut Vec<CubicBez>, closed: bool, contours: &mut Vec<Contour>| {
        if !segments.is_empty() {
            contours.push(Contour {
                segments: std::mem::take(segments),
                closed,
            });
        }
    };
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(point) => {
                finish(&mut segments, false, &mut contours);
                start = point;
                current = point;
            }
            PathEl::LineTo(point) => {
                segments.push(PathSeg::Line(kurbo::Line::new(current, point)).to_cubic());
                current = point;
            }
            PathEl::QuadTo(control, point) => {
                segments
                    .push(PathSeg::Quad(kurbo::QuadBez::new(current, control, point)).to_cubic());
                current = point;
            }
            PathEl::CurveTo(first, second, point) => {
                segments.push(CubicBez::new(current, first, second, point));
                current = point;
            }
            PathEl::ClosePath => {
                if current.distance(start) > 1e-12 {
                    segments.push(PathSeg::Line(kurbo::Line::new(current, start)).to_cubic());
                }
                finish(&mut segments, true, &mut contours);
                current = start;
            }
        }
    }
    finish(&mut segments, false, &mut contours);
    contours
}

fn write_cubics(path: &mut BezPath, segments: &[CubicBez], closed: bool) {
    let Some(first) = segments.first() else {
        return;
    };
    path.move_to(first.p0);
    for segment in segments {
        path.curve_to(segment.p1, segment.p2, segment.p3);
    }
    if closed {
        path.close_path();
    }
}

fn is_straight(segment: &CubicBez) -> bool {
    let chord = segment.p3 - segment.p0;
    let length = chord.hypot();
    if length < 1e-12 {
        return true;
    }
    let off = |point: Point| (point - segment.p0).cross(chord).abs() / length;
    off(segment.p1) < 1e-9 && off(segment.p2) < 1e-9
}

/// Unit normal at `t`, to the left of the direction of travel.
fn normal(segment: &CubicBez, t: f64) -> Vec2 {
    let mut tangent = segment.deriv().eval(t).to_vec2();
    if tangent.hypot() < 1e-12 {
        tangent = segment.p3 - segment.p0;
    }
    let length = tangent.hypot();
    if length < 1e-12 {
        return Vec2::ZERO;
    }
    Vec2::new(-tangent.y, tangent.x) / length
}

/// `count` points evenly spaced by arc length along `segment`, excluding
/// its start and including its end, with their normals.
fn samples(segment: &CubicBez, count: usize) -> Vec<(Point, Vec2)> {
    let length = segment.arclen(ARCLEN_ACCURACY);
    (1..=count)
        .map(|index| {
            let t = if length < 1e-12 {
                index as f64 / count as f64
            } else {
                segment.inv_arclen(length * index as f64 / count as f64, ARCLEN_ACCURACY)
            };
            (segment.eval(t), normal(segment, t))
        })
        .collect()
}

/// Joins `points` with straight lines, or with a Catmull-Rom curve through
/// them when `smooth`.
fn polyline(path: &mut BezPath, points: &[Point], closed: bool, smooth: bool) {
    let Some(&first) = points.first() else {
        return;
    };
    path.move_to(first);
    let count = points.len();
    if !smooth || count < 3 {
        for &point in &points[1..] {
            path.line_to(point);
        }
    } else {
        let at = |index: isize| -> Point {
            if closed {
                points[index.rem_euclid(count as isize) as usize]
            } else {
                points[index.clamp(0, count as isize - 1) as usize]
            }
        };
        let last = if closed { count } else { count - 1 };
        for index in 0..last as isize {
            let (p0, p1, p2, p3) = (at(index - 1), at(index), at(index + 1), at(index + 2));
            path.curve_to(p1 + (p2 - p0) / 6.0, p2 - (p3 - p1) / 6.0, p2);
        }
        if closed {
            path.close_path();
        }
        return;
    }
    if closed {
        path.close_path();
    }
}

/// Zig zag: `ridges` peaks on every segment, alternately `size` to each
/// side of the path; vertices stay in place. `smooth` waves instead of
/// cornering.
pub fn zigzag(path: &BezPath, size: f64, ridges: u32, smooth: bool) -> BezPath {
    let ridges = ridges.max(1) as usize;
    let mut result = BezPath::new();
    for contour in contours(path) {
        let mut points = vec![contour.segments[0].p0];
        for segment in &contour.segments {
            let count = 2 * ridges;
            for (index, (point, normal)) in samples(segment, count).into_iter().enumerate() {
                let sign = if index + 1 == count {
                    0.0
                } else if index % 2 == 0 {
                    1.0
                } else {
                    -1.0
                };
                points.push(point + normal * size * sign);
            }
        }
        if contour.closed {
            points.pop();
        }
        polyline(&mut result, &points, contour.closed, smooth);
    }
    result
}

/// Rounds every corner between two straight segments with an arc of
/// `radius`, shortened where a side is too short for it.
pub fn round_corners(path: &BezPath, radius: f64) -> BezPath {
    if radius <= 0.0 {
        return path.clone();
    }
    let mut result = BezPath::new();
    for contour in contours(path) {
        let segments = &contour.segments;
        let count = segments.len();
        // Trimmed ends of each segment and the corner after it.
        let mut cut_start = vec![0.0; count];
        let mut cut_end = vec![0.0; count];
        let mut corners: Vec<Option<(Point, Point, Point)>> = vec![None; count];
        let corner_count = if contour.closed {
            count
        } else {
            count.saturating_sub(1)
        };
        for index in 0..corner_count {
            let next = (index + 1) % count;
            let (incoming, outgoing) = (&segments[index], &segments[next]);
            if !is_straight(incoming) || !is_straight(outgoing) {
                continue;
            }
            let vertex = incoming.p3;
            let into = incoming.p3 - incoming.p0;
            let away = outgoing.p3 - outgoing.p0;
            let (into_length, away_length) = (into.hypot(), away.hypot());
            if into_length < 1e-12 || away_length < 1e-12 {
                continue;
            }
            let (into, away) = (into / into_length, away / away_length);
            if into.cross(away).abs() < 1e-9 && into.dot(away) > 0.0 {
                continue;
            }
            let cut = radius.min(into_length / 2.0).min(away_length / 2.0);
            cut_end[index] = cut;
            cut_start[next] = cut;
            corners[index] = Some((vertex - into * cut, vertex, vertex + away * cut));
        }
        let trimmed: Vec<CubicBez> = segments
            .iter()
            .enumerate()
            .map(|(index, segment)| {
                if cut_start[index] == 0.0 && cut_end[index] == 0.0 {
                    return *segment;
                }
                let direction = (segment.p3 - segment.p0).normalize();
                let from = segment.p0 + direction * cut_start[index];
                let to = segment.p3 - direction * cut_end[index];
                PathSeg::Line(kurbo::Line::new(from, to)).to_cubic()
            })
            .collect();
        let mut output = Vec::with_capacity(count * 2);
        for index in 0..count {
            output.push(trimmed[index]);
            if let Some((from, vertex, to)) = corners[index] {
                output.push(CubicBez::new(
                    from,
                    from + (vertex - from) * KAPPA,
                    to + (vertex - to) * KAPPA,
                    to,
                ));
            }
        }
        // A closed contour whose first corner rounds starts after it.
        write_cubics(&mut result, &output, contour.closed);
    }
    result
}

/// Average of the path's vertices, the center pucker, bloat and twist use.
fn vertex_center(contours: &[Contour]) -> Point {
    let mut sum = Vec2::ZERO;
    let mut count = 0.0;
    for contour in contours {
        for segment in &contour.segments {
            sum += segment.p0.to_vec2();
            count += 1.0;
        }
        if !contour.closed
            && let Some(last) = contour.segments.last()
        {
            sum += last.p3.to_vec2();
            count += 1.0;
        }
    }
    if count == 0.0 {
        Point::ORIGIN
    } else {
        (sum / count).to_point()
    }
}

/// Pucker (negative `amount`) pushes vertices out and pulls the curves
/// between them in, into spikes; bloat (positive) the reverse, into
/// rounded lobes. `amount` is a fraction of each point's distance to the
/// center, from -1 to 1.
pub fn pucker_bloat(path: &BezPath, amount: f64) -> BezPath {
    let contours = contours(path);
    let center = vertex_center(&contours);
    let vertex = |point: Point| point + (center - point) * amount;
    let control = |point: Point| point - (center - point) * amount;
    let mut result = BezPath::new();
    for contour in &contours {
        let segments: Vec<CubicBez> = contour
            .segments
            .iter()
            .map(|segment| {
                CubicBez::new(
                    vertex(segment.p0),
                    control(segment.p1),
                    control(segment.p2),
                    vertex(segment.p3),
                )
            })
            .collect();
        write_cubics(&mut result, &segments, contour.closed);
    }
    result
}

/// Pieces each segment is cut into before a twist bends it.
const TWIST_PIECES: usize = 8;

/// Twists the path about its center by `angle` radians at the center,
/// fading to no turn at its farthest vertex.
pub fn twist(path: &BezPath, angle: f64) -> BezPath {
    let contours = contours(path);
    let center = vertex_center(&contours);
    let reach = contours
        .iter()
        .flat_map(|contour| contour.segments.iter())
        .flat_map(|segment| [segment.p0, segment.p3])
        .map(|point| point.distance(center))
        .fold(0.0, f64::max);
    if reach < 1e-12 || angle == 0.0 {
        return path.clone();
    }
    let turn = |point: Point| {
        let offset = point - center;
        let amount = angle * (1.0 - offset.hypot() / reach).max(0.0);
        let (sin, cos) = amount.sin_cos();
        center
            + Vec2::new(
                offset.x * cos - offset.y * sin,
                offset.x * sin + offset.y * cos,
            )
    };
    let mut result = BezPath::new();
    for contour in &contours {
        let mut segments = Vec::with_capacity(contour.segments.len() * TWIST_PIECES);
        for segment in &contour.segments {
            for piece in 0..TWIST_PIECES {
                let part = segment.subsegment(
                    piece as f64 / TWIST_PIECES as f64..(piece + 1) as f64 / TWIST_PIECES as f64,
                );
                segments.push(CubicBez::new(
                    turn(part.p0),
                    turn(part.p1),
                    turn(part.p2),
                    turn(part.p3),
                ));
            }
        }
        write_cubics(&mut result, &segments, contour.closed);
    }
    result
}

/// Wiggle: `detail` points on every segment, each pushed along the path's
/// normal by seeded noise of up to `size`, changing `frequency` times a
/// second at `time`. The points are joined smoothly.
pub fn wiggle(
    path: &BezPath,
    size: f64,
    detail: u32,
    frequency: f64,
    seed: u64,
    time: f64,
) -> BezPath {
    let detail = detail.max(1) as usize;
    let noise = Noise::new(seed, 1.0, 1.0, 2);
    let mut result = BezPath::new();
    for (contour_index, contour) in contours(path).into_iter().enumerate() {
        let first = &contour.segments[0];
        let mut points = vec![(first.p0, normal(first, 0.0))];
        for segment in &contour.segments {
            points.extend(samples(segment, detail));
        }
        if contour.closed {
            points.pop();
        }
        let channel = contour_index as f64 * 17.3;
        let displaced: Vec<Point> = points
            .iter()
            .enumerate()
            .map(|(index, (point, normal))| {
                let offset = noise.sample(index as f64 * 0.61 + channel, time * frequency);
                *point + *normal * size * offset
            })
            .collect();
        polyline(&mut result, &displaced, contour.closed, true);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    fn square() -> BezPath {
        kurbo::Rect::new(-1.0, -1.0, 1.0, 1.0).to_path(1e-3)
    }

    fn points(path: &BezPath) -> Vec<Point> {
        path.elements()
            .iter()
            .filter_map(|element| element.end_point())
            .collect()
    }

    #[test]
    fn zigzag_keeps_vertices_and_alternates_sides() {
        let line: BezPath = {
            let mut path = BezPath::new();
            path.move_to((0.0, 0.0));
            path.line_to((4.0, 0.0));
            path
        };
        let zz = zigzag(&line, 0.5, 2, false);
        let ys: Vec<f64> = points(&zz).iter().map(|point| point.y).collect();
        assert_eq!(ys.len(), 5);
        assert!((ys[0]).abs() < 1e-9 && (ys[4]).abs() < 1e-9);
        assert!((ys[1].abs() - 0.5).abs() < 1e-9);
        assert!(ys[1] * ys[2] < 0.0 && ys[2] * ys[3] < 0.0);
        // At size 0 the square's outline is unchanged.
        let flat = zigzag(&square(), 0.0, 3, false);
        assert!((flat.area() - square().area()).abs() < 1e-9);
        assert!(
            points(&flat)
                .iter()
                .any(|point| point.distance(Point::new(1.0, 1.0)) < 1e-9)
        );
    }

    #[test]
    fn round_corners_cut_each_corner_by_the_radius() {
        let rounded = round_corners(&square(), 0.25);
        let area = rounded.area().abs();
        // Four corners lose (1 - π/4) r² each, approximately.
        let expected = 4.0 - 4.0 * (1.0 - std::f64::consts::FRAC_PI_4) * 0.0625;
        assert!((area - expected).abs() < 1e-3, "{area} vs {expected}");
        assert!(!points(&rounded).contains(&Point::new(1.0, 1.0)));
        // A radius beyond half a side is limited to it: a circle-ish shape.
        let big = round_corners(&square(), 5.0).area().abs();
        assert!((big - std::f64::consts::PI).abs() < 0.01, "{big}");
        // Curves keep their vertices.
        let circle = kurbo::Circle::new((0.0, 0.0), 1.0).to_path(1e-3);
        assert!((round_corners(&circle, 0.3).area() - circle.area()).abs() < 1e-9);
    }

    #[test]
    fn pucker_spikes_and_bloat_puffs() {
        // Pucker: corners out, edges pinched in to x = 0.8 at their middle.
        let puckered = pucker_bloat(&square(), -0.4);
        assert!(points(&puckered).contains(&Point::new(1.4, 1.4)));
        assert_eq!(puckered.winding(Point::new(0.95, 0.0)), 0);
        // Bloat: corners in, edges bulging out to x = 1.2.
        let bloated = pucker_bloat(&square(), 0.4);
        assert_ne!(bloated.winding(Point::new(1.15, 0.0)), 0);
        // Vertices move toward the center when bloated.
        assert!(points(&pucker_bloat(&square(), 0.5)).contains(&Point::new(0.5, 0.5)));
        assert!((pucker_bloat(&square(), 0.0).area() - square().area()).abs() < 1e-9);
    }

    #[test]
    fn twist_turns_the_middle_and_leaves_the_rim() {
        let square = square();
        let twisted = twist(&square, 1.0);
        // The corners are the farthest points: they stay.
        for corner in [(1.0, 1.0), (-1.0, -1.0)] {
            assert!(
                points(&twisted)
                    .iter()
                    .any(|point| point.distance(Point::new(corner.0, corner.1)) < 1e-9)
            );
        }
        // Mid-edge points turn.
        assert!(
            !points(&twisted)
                .iter()
                .any(|point| point.distance(Point::new(1.0, 0.0)) < 1e-6)
        );
        assert_eq!(twist(&square, 0.0), square);
    }

    #[test]
    fn wiggle_is_a_pure_function_of_time_and_seed() {
        let circle = kurbo::Circle::new((0.0, 0.0), 1.0).to_path(1e-3);
        let a = wiggle(&circle, 0.1, 6, 2.0, 3, 0.5);
        assert_eq!(a, wiggle(&circle, 0.1, 6, 2.0, 3, 0.5));
        assert_ne!(a, wiggle(&circle, 0.1, 6, 2.0, 3, 0.75));
        assert_ne!(a, wiggle(&circle, 0.1, 6, 2.0, 4, 0.5));
        // Stays within size of the outline.
        for point in points(&a) {
            assert!((point.distance(Point::ORIGIN) - 1.0).abs() <= 0.1 + 1e-6);
        }
        // Size 0 lies on the outline.
        for point in points(&wiggle(&circle, 0.0, 6, 2.0, 3, 0.5)) {
            assert!((point.distance(Point::ORIGIN) - 1.0).abs() < 1e-3);
        }
    }
}
