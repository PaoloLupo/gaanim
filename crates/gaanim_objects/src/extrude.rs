//! Extrusion of 2D outlines into closed 3D meshes (CA-06): a front and a back
//! cap, side walls along every contour and an optional 45° bevel.
//!
//! The outline is flattened and normalized under the nonzero fill rule, as
//! it is drawn, so letters keep their holes and overlapping contours merge.
//! The mesh is centered on the outline's box and spans `z ∈ [-depth/2,
//! depth/2]`, the front cap facing +z. Triangles are ordered walls, bevels,
//! back cap, front cap: the renderer draws the faces of a solid in index
//! order, and the caps must cover the walls of concave outlines.

use gaanim_core::glam::DVec2;
use gaanim_core::kurbo::{BezPath, Point};
use gaanim_scene::{Material3D, TriangleMeshData};
use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::simplify::SimplifyShape;

use crate::boolean::bezpath_to_shape_with_tolerance;
use crate::earcut::earcut;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExtrudeError {
    #[error("extrusion depth must be finite and greater than zero")]
    InvalidDepth,
    #[error("bevel must be finite, non-negative and less than half the depth")]
    InvalidBevel,
    #[error("tolerance must be finite and greater than zero")]
    InvalidTolerance,
    #[error("the outline encloses no area to extrude")]
    EmptyShape,
}

/// Turns sharper than this keep separate wall normals on each side, so
/// corners stay crisp; gentler ones (flattened curves) share a smooth normal.
const SMOOTH_ANGLE: f64 = 35.0 * std::f64::consts::PI / 180.0;
/// Longest miter of a bevel corner, in bevel widths.
const MAX_MITER: f64 = 3.0;

/// An extruded mesh and the center of the outline's box, where the mesh
/// belongs in the outline's plane.
#[derive(Debug, Clone)]
pub struct Extrusion {
    pub mesh: TriangleMeshData,
    pub center: Point,
}

/// Extrude the area `path` fills to `depth`, with a `bevel` chamfer on both
/// caps, flattening curves to `tolerance`.
pub fn extrude(
    path: &BezPath,
    depth: f64,
    bevel: f64,
    tolerance: f64,
    material: Material3D,
) -> Result<Extrusion, ExtrudeError> {
    if !depth.is_finite() || depth <= 0.0 {
        return Err(ExtrudeError::InvalidDepth);
    }
    if !bevel.is_finite() || bevel < 0.0 || bevel >= depth / 2.0 {
        return Err(ExtrudeError::InvalidBevel);
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(ExtrudeError::InvalidTolerance);
    }
    let shapes: Vec<Vec<Vec<DVec2>>> = bezpath_to_shape_with_tolerance(path, tolerance)
        .simplify_shape(FillRule::NonZero)
        .into_iter()
        .filter_map(|shape| clean_shape(shape, tolerance))
        .collect();
    let points = shapes.iter().flatten().flatten();
    let (min, max) = points.fold(
        (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    if shapes.is_empty() || !(max.x > min.x && max.y > min.y) {
        return Err(ExtrudeError::EmptyShape);
    }
    let center = (min + max) / 2.0;
    let mut builder = MeshBuilder {
        min: min - center,
        size: max - min,
        depth,
        ..MeshBuilder::default()
    };
    let half = depth / 2.0;
    let mut caps = Vec::with_capacity(shapes.len());
    for shape in &shapes {
        let shape: Vec<Vec<DVec2>> = shape
            .iter()
            .map(|contour| contour.iter().map(|point| *point - center).collect())
            .collect();
        let inset = inset_shape(&shape, bevel);
        let side = half - if inset.is_some() { bevel } else { 0.0 };
        for contour in &shape {
            builder.walls(contour, -side, side);
        }
        if let Some(inset) = &inset {
            for (contour, inner) in shape.iter().zip(inset) {
                builder.bevel(contour, inner, side, half);
                builder.bevel(contour, inner, -side, -half);
            }
        }
        caps.push(inset.unwrap_or(shape));
    }
    for cap in &caps {
        builder.cap(cap, -half);
    }
    for cap in &caps {
        builder.cap(cap, half);
    }
    if builder.indices.is_empty() {
        return Err(ExtrudeError::EmptyShape);
    }
    Ok(Extrusion {
        mesh: TriangleMeshData {
            vertices: builder.vertices,
            indices: builder.indices,
            normals: Some(builder.normals),
            uvs: Some(builder.uvs),
            color: None,
            colors: None,
            material: Some(material),
        },
        center: Point::new(center.x, center.y),
    })
}

/// Contours of one shape without repeated or collinear points, the outer one
/// counterclockwise and holes clockwise; `None` when the outer one vanishes.
fn clean_shape(shape: Vec<Vec<[f64; 2]>>, tolerance: f64) -> Option<Vec<Vec<DVec2>>> {
    let mut contours = Vec::with_capacity(shape.len());
    for (index, contour) in shape.into_iter().enumerate() {
        let mut points: Vec<DVec2> = Vec::with_capacity(contour.len());
        for [x, y] in contour {
            let point = DVec2::new(x, y);
            if points
                .last()
                .is_none_or(|last| last.distance(point) > tolerance * 0.01)
            {
                points.push(point);
            }
        }
        while points.len() > 1 && points[0].distance(points[points.len() - 1]) <= tolerance * 0.01 {
            points.pop();
        }
        // Drop points on a straight line between their neighbors.
        let mut changed = true;
        while changed && points.len() >= 3 {
            changed = false;
            for i in 0..points.len() {
                let n = points.len();
                let (a, b, c) = (points[(i + n - 1) % n], points[i], points[(i + 1) % n]);
                let span = (c - a).length().max(1e-12);
                if ((b - a).perp_dot(c - a) / span).abs() <= tolerance * 1e-3 {
                    points.remove(i);
                    changed = true;
                    break;
                }
            }
        }
        if points.len() < 3 {
            if index == 0 {
                return None;
            }
            continue;
        }
        let ccw = signed_area(&points) > 0.0;
        if ccw != (index == 0) {
            points.reverse();
        }
        contours.push(points);
    }
    Some(contours)
}

fn signed_area(points: &[DVec2]) -> f64 {
    let n = points.len();
    (0..n)
        .map(|i| points[i].perp_dot(points[(i + 1) % n]))
        .sum::<f64>()
        / 2.0
}

/// Unit normal of the edge from `a` to `b` pointing away from the material,
/// which lies on the left of every contour.
fn outward(a: DVec2, b: DVec2) -> DVec2 {
    let d = (b - a).normalize_or_zero();
    DVec2::new(d.y, -d.x)
}

/// Every contour of `shape` moved `bevel` into the material, one point per
/// point; `None` without a bevel, or when moving would fold a contour (a
/// stem thinner than twice the bevel), and the shape then gets none.
fn inset_shape(shape: &[Vec<DVec2>], bevel: f64) -> Option<Vec<Vec<DVec2>>> {
    if bevel <= 0.0 {
        return None;
    }
    let mut inset = Vec::with_capacity(shape.len());
    for contour in shape {
        let n = contour.len();
        let moved: Vec<DVec2> = (0..n)
            .map(|i| {
                let (prev, point, next) =
                    (contour[(i + n - 1) % n], contour[i], contour[(i + 1) % n]);
                let (a, b) = (outward(prev, point), outward(point, next));
                let mut miter = (a + b) / (1.0 + a.dot(b)).max(1e-6);
                if miter.length() > MAX_MITER {
                    miter = miter.normalize() * MAX_MITER;
                }
                point - miter * bevel
            })
            .collect();
        // Every edge must keep its direction and the contour its winding.
        let folded = (0..n).any(|i| {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            let (c, d) = (moved[i], moved[(i + 1) % n]);
            (b - a).dot(d - c) <= 0.0
        });
        let area = signed_area(&moved);
        if folded || area.signum() != signed_area(contour).signum() || area.abs() < 1e-12 {
            return None;
        }
        inset.push(moved);
    }
    Some(inset)
}

/// Whether the contour turns gently enough at point `i` to share a normal.
fn smooth_at(contour: &[DVec2], i: usize) -> bool {
    let n = contour.len();
    let (prev, point, next) = (contour[(i + n - 1) % n], contour[i], contour[(i + 1) % n]);
    let (a, b) = (
        (point - prev).normalize_or_zero(),
        (next - point).normalize_or_zero(),
    );
    a.dot(b) >= SMOOTH_ANGLE.cos()
}

/// The 2D outward normal a vertex of edge `i`'s start (`end = false`) or end
/// shows: shared with the neighboring edge at a smooth point.
fn vertex_normal(contour: &[DVec2], edge: usize, end: bool) -> DVec2 {
    let n = contour.len();
    let own = outward(contour[edge], contour[(edge + 1) % n]);
    let (point, other) = if end {
        let point = (edge + 1) % n;
        (point, outward(contour[point], contour[(point + 1) % n]))
    } else {
        let previous = (edge + n - 1) % n;
        (edge, outward(contour[previous], contour[edge]))
    };
    if smooth_at(contour, point) {
        (own + other).normalize_or(own)
    } else {
        own
    }
}

#[derive(Default)]
struct MeshBuilder {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
    /// Box of the centered outline, for cap texture coordinates.
    min: DVec2,
    size: DVec2,
    depth: f64,
}

impl MeshBuilder {
    fn vertex(&mut self, point: DVec2, z: f64, normal: [f64; 3], u: f64) -> u32 {
        let normal = gaanim_core::glam::DVec3::from_array(normal).normalize_or_zero();
        self.vertices
            .push([point.x as f32, point.y as f32, z as f32]);
        self.normals
            .push([normal.x as f32, normal.y as f32, normal.z as f32]);
        let v = (z / self.depth + 0.5).clamp(0.0, 1.0);
        self.uvs.push([u.clamp(0.0, 1.0) as f32, v as f32]);
        (self.vertices.len() - 1) as u32
    }

    /// Two triangles over `a b c d` (counterclockwise seen from outside).
    fn quad(&mut self, [a, b, c, d]: [u32; 4]) {
        self.indices.extend([a, b, c, a, c, d]);
    }

    fn walls(&mut self, contour: &[DVec2], bottom: f64, top: f64) {
        if top <= bottom {
            return;
        }
        let n = contour.len();
        let perimeter: f64 = (0..n)
            .map(|i| contour[i].distance(contour[(i + 1) % n]))
            .sum();
        let mut along = 0.0;
        for i in 0..n {
            let (a, b) = (contour[i], contour[(i + 1) % n]);
            let next_along = along + a.distance(b);
            let (start, end) = (
                vertex_normal(contour, i, false),
                vertex_normal(contour, i, true),
            );
            let (u0, u1) = (along / perimeter, next_along / perimeter);
            let quad = [
                self.vertex(a, bottom, [start.x, start.y, 0.0], u0),
                self.vertex(b, bottom, [end.x, end.y, 0.0], u1),
                self.vertex(b, top, [end.x, end.y, 0.0], u1),
                self.vertex(a, top, [start.x, start.y, 0.0], u0),
            ];
            self.quad(quad);
            along = next_along;
        }
    }

    /// A 45° chamfer from `contour` at `z_outer` to `inner` at `z_inner`.
    fn bevel(&mut self, contour: &[DVec2], inner: &[DVec2], z_outer: f64, z_inner: f64) {
        let n = contour.len();
        let up = (z_inner - z_outer).signum();
        let normal = |side: DVec2| [side.x, side.y, up];
        for i in 0..n {
            let j = (i + 1) % n;
            let (start, end) = (
                vertex_normal(contour, i, false),
                vertex_normal(contour, i, true),
            );
            let u = |k: usize| k as f64 / n as f64;
            let outer_a = self.vertex(contour[i], z_outer, normal(start), u(i));
            let outer_b = self.vertex(contour[j], z_outer, normal(end), u(i + 1));
            let inner_b = self.vertex(inner[j], z_inner, normal(end), u(i + 1));
            let inner_a = self.vertex(inner[i], z_inner, normal(start), u(i));
            // Seen from outside, the front chamfer runs outer → inner upward
            // and the back one the other way round.
            if up > 0.0 {
                self.quad([outer_a, outer_b, inner_b, inner_a]);
            } else {
                self.quad([inner_a, inner_b, outer_b, outer_a]);
            }
        }
    }

    /// A flat cap at `z` over the contours of one shape, facing away from
    /// the mesh.
    fn cap(&mut self, shape: &[Vec<DVec2>], z: f64) {
        let mut points = Vec::new();
        let mut holes = Vec::new();
        for (index, contour) in shape.iter().enumerate() {
            if index > 0 {
                holes.push(points.len());
            }
            points.extend(contour.iter().map(|point| [point.x, point.y]));
        }
        let triangles = earcut(&points, &holes);
        let base = self.vertices.len() as u32;
        let facing = z.signum();
        for point in &points {
            let point = DVec2::from_array(*point);
            let uv = (point - self.min) / self.size;
            self.vertex(point, z, [0.0, 0.0, facing], uv.x);
            let last = self.uvs.len() - 1;
            self.uvs[last][1] = uv.y.clamp(0.0, 1.0) as f32;
        }
        for triangle in triangles.chunks_exact(3) {
            let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| base + i as u32);
            let (pa, pb, pc) = (
                DVec2::from_array(points[triangle[0]]),
                DVec2::from_array(points[triangle[1]]),
                DVec2::from_array(points[triangle[2]]),
            );
            // Counterclockwise from +z faces the front; the back cap turns over.
            if ((pb - pa).perp_dot(pc - pa) > 0.0) == (facing > 0.0) {
                self.indices.extend([a, b, c]);
            } else {
                self.indices.extend([a, c, b]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::Vec3;
    use gaanim_core::kurbo::Shape;
    use std::collections::HashMap;

    fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> BezPath {
        gaanim_core::kurbo::Rect::new(x0, y0, x1, y1).to_path(0.1)
    }

    fn assert_valid(mesh: &TriangleMeshData) {
        let normals = mesh.normals.as_ref().unwrap();
        assert_eq!(normals.len(), mesh.vertices.len());
        assert_eq!(mesh.uvs.as_ref().unwrap().len(), mesh.vertices.len());
        assert!(
            normals
                .iter()
                .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-5)
        );
        assert!(
            mesh.uvs
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .all(|uv| (0.0..=1.0).contains(uv))
        );
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] =
                [0, 1, 2].map(|k| Vec3::from_array(mesh.vertices[triangle[k] as usize]));
            let face = (b - a).cross(c - a);
            if face.length_squared() < 1e-12 {
                continue;
            }
            let expected = (0..3)
                .map(|k| Vec3::from_array(normals[triangle[k] as usize]))
                .sum::<Vec3>()
                .normalize_or_zero();
            assert!(
                face.normalize().dot(expected) > 0.5,
                "{face:?} vs {expected:?}"
            );
        }
    }

    /// Every edge, by position, is shared by exactly two triangles in
    /// opposite directions: the surface is closed and consistently wound.
    fn assert_closed(mesh: &TriangleMeshData) {
        let key = |i: u32| {
            let v = mesh.vertices[i as usize];
            [v[0], v[1], v[2]].map(|c| (c * 1e4).round() as i64)
        };
        let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
        for triangle in mesh.indices.chunks_exact(3) {
            for k in 0..3 {
                let (a, b) = (key(triangle[k]), key(triangle[(k + 1) % 3]));
                *edges.entry((a, b)).or_default() += 1;
            }
        }
        for (&(a, b), &count) in &edges {
            assert_eq!(
                edges.get(&(b, a)).copied().unwrap_or(0),
                count,
                "{a:?} -> {b:?}"
            );
        }
    }

    #[test]
    fn a_box_extrudes_into_a_closed_prism() {
        let extrusion = extrude(
            &rect(1.0, 1.0, 3.0, 2.0),
            0.5,
            0.0,
            0.01,
            Material3D::default(),
        )
        .unwrap();
        assert_eq!(extrusion.center, Point::new(2.0, 1.5));
        let mesh = &extrusion.mesh;
        assert_valid(mesh);
        assert_closed(mesh);
        // Four walls and two caps of two triangles each.
        assert_eq!(mesh.indices.len(), 3 * (8 + 4));
        let zs: Vec<f32> = mesh.vertices.iter().map(|v| v[2]).collect();
        assert!(zs.iter().all(|z| (z.abs() - 0.25).abs() < 1e-6));
        // The front cap comes last, facing the camera.
        let last = mesh.indices[mesh.indices.len() - 1] as usize;
        assert_eq!(mesh.normals.as_ref().unwrap()[last], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn holes_and_bevels_stay_closed() {
        let mut ring = rect(-2.0, -2.0, 2.0, 2.0);
        let mut hole = rect(-1.0, -1.0, 1.0, 1.0);
        // A hole in the opposite winding, as a letter "O" has.
        hole = hole.reverse_subpaths();
        ring.extend(hole);
        for bevel in [0.0, 0.05] {
            let mesh = extrude(&ring, 0.4, bevel, 0.01, Material3D::default())
                .unwrap()
                .mesh;
            assert_valid(&mesh);
            assert_closed(&mesh);
            // Nothing covers the hole.
            let inside = mesh
                .vertices
                .iter()
                .any(|v| v[0].abs() < 0.9 && v[1].abs() < 0.9);
            assert!(!inside);
        }
        let circle = gaanim_core::kurbo::Circle::new((0.0, 0.0), 1.0).to_path(0.01);
        let mesh = extrude(&circle, 0.3, 0.05, 0.01, Material3D::default())
            .unwrap()
            .mesh;
        assert_valid(&mesh);
        assert_closed(&mesh);
        // The round wall is smooth: neighboring wall vertices share normals.
        let normals = mesh.normals.as_ref().unwrap();
        let flat_walls = mesh
            .indices
            .chunks_exact(3)
            .filter(|t| t.iter().all(|i| normals[*i as usize][2] == 0.0))
            .filter(|t| {
                normals[t[0] as usize] == normals[t[1] as usize]
                    && normals[t[1] as usize] == normals[t[2] as usize]
            })
            .count();
        assert_eq!(flat_walls, 0);
    }

    #[test]
    fn bevels_chamfer_the_caps_and_give_up_on_thin_stems() {
        let mesh = extrude(
            &rect(0.0, 0.0, 2.0, 1.0),
            0.4,
            0.1,
            0.01,
            Material3D::default(),
        )
        .unwrap()
        .mesh;
        // The front cap is inset by the bevel.
        let front: Vec<[f32; 3]> = mesh
            .vertices
            .iter()
            .zip(mesh.normals.as_ref().unwrap())
            .filter(|(_, n)| **n == [0.0, 0.0, 1.0])
            .map(|(v, _)| *v)
            .collect();
        assert!(
            front
                .iter()
                .all(|v| v[0].abs() <= 0.9 + 1e-6 && v[1].abs() <= 0.4 + 1e-6)
        );
        // A stem thinner than twice the bevel keeps square edges.
        let thin = extrude(
            &rect(0.0, 0.0, 2.0, 0.1),
            0.4,
            0.1,
            0.01,
            Material3D::default(),
        )
        .unwrap()
        .mesh;
        assert_closed(&thin);
        assert_eq!(thin.indices.len(), 3 * (8 + 4));
    }

    #[test]
    fn invalid_input_is_rejected() {
        let square = rect(0.0, 0.0, 1.0, 1.0);
        let material = Material3D::default();
        assert_eq!(
            extrude(&square, 0.0, 0.0, 0.01, material).unwrap_err(),
            ExtrudeError::InvalidDepth
        );
        assert_eq!(
            extrude(&square, 0.2, 0.1, 0.01, material).unwrap_err(),
            ExtrudeError::InvalidBevel
        );
        assert_eq!(
            extrude(&square, 0.2, -0.1, 0.01, material).unwrap_err(),
            ExtrudeError::InvalidBevel
        );
        assert_eq!(
            extrude(&square, 0.2, 0.0, 0.0, material).unwrap_err(),
            ExtrudeError::InvalidTolerance
        );
        let mut line = BezPath::new();
        line.move_to((0.0, 0.0));
        line.line_to((1.0, 0.0));
        assert_eq!(
            extrude(&line, 0.2, 0.0, 0.01, material).unwrap_err(),
            ExtrudeError::EmptyShape
        );
        assert_eq!(
            extrude(&BezPath::new(), 0.2, 0.0, 0.01, material).unwrap_err(),
            ExtrudeError::EmptyShape
        );
    }
}
