//! Native deterministic geometry for friendly Y-up 3D primitives.

use std::f64::consts::TAU;

use gaanim_core::glam::{DQuat, DVec3, Vec2, Vec3};
use gaanim_scene::{Material3D, TriangleMeshData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Primitive3DError {
    #[error("3D primitive dimensions must be finite and greater than zero")]
    InvalidDimension,
    #[error("3D primitive segments must be at least 3")]
    InvalidSegments,
    #[error("sphere rings must be at least 2")]
    InvalidRings,
    #[error("torus rings must be at least 3")]
    InvalidTorusRings,
    #[error("plane subdivisions must both be at least 1")]
    InvalidSubdivisions,
    #[error("the torus minor radius must be less than its major radius")]
    InvalidTorus,
    #[error("3D primitive points must be finite")]
    InvalidPoint,
    #[error("a tube needs at least two distinct points, three when closed")]
    TooFewPoints,
    #[error("an arrow's start and end must be distinct")]
    ZeroLengthArrow,
    #[error("an arrow's head must be shorter than the arrow and wider than its shaft")]
    InvalidArrowHead,
}

fn dimension(value: f64) -> Result<f32, Primitive3DError> {
    (value.is_finite() && value > 0.0)
        .then_some(value as f32)
        .ok_or(Primitive3DError::InvalidDimension)
}

fn positive(value: f64) -> Result<f64, Primitive3DError> {
    dimension(value).map(|_| value)
}

/// A mesh built around its own origin, and the point where that origin
/// belongs: the drawable sits there so it moves, turns and grows about it.
#[derive(Debug, Clone)]
pub struct PlacedMesh {
    pub mesh: TriangleMeshData,
    pub center: DVec3,
}

/// Sizes of a 3D arrow, in scene units; `None` takes the default, relative
/// to the arrow's length.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ArrowSizes {
    /// Radius of the shaft; 2.5% of the length by default.
    pub shaft_radius: Option<f64>,
    /// Radius of the head's base; three times the shaft's by default.
    pub head_radius: Option<f64>,
    /// Length of the head; 22% of the length by default.
    pub head_length: Option<f64>,
}

/// Turns sharper than this give a tube a crease, with separate normals on
/// each side; gentler ones (sampled curves) stay smooth.
const TUBE_SMOOTH_ANGLE: f64 = 35.0 * std::f64::consts::PI / 180.0;
/// Longest a tube's cross-section stretches at a corner, in radii.
const TUBE_MAX_MITER: f64 = 3.0;

/// Vertices and triangles of a mesh being built from rings of vertices.
#[derive(Default)]
struct RingBuilder {
    vertices: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    indices: Vec<u32>,
}

impl RingBuilder {
    fn vertex(&mut self, position: DVec3, normal: DVec3, [u, v]: [f64; 2]) -> u32 {
        let normal = normal.normalize_or_zero();
        self.vertices.push(position.as_vec3().to_array());
        self.normals.push(normal.as_vec3().to_array());
        self.uvs
            .push([u.clamp(0.0, 1.0) as f32, v.clamp(0.0, 1.0) as f32]);
        (self.vertices.len() - 1) as u32
    }

    /// A ring of `segments + 1` vertices, the last one closing the seam:
    /// `place(j)` gives vertex `j`'s position, normal and texture coordinates.
    fn ring(
        &mut self,
        segments: u32,
        mut place: impl FnMut(u32) -> (DVec3, DVec3, [f64; 2]),
    ) -> u32 {
        let first = self.vertices.len() as u32;
        for j in 0..=segments {
            let (position, normal, uv) = place(j);
            self.vertex(position, normal, uv);
        }
        first
    }

    /// Quads between two rings of `segments + 1` vertices running the same
    /// way. Rings around an axis, counterclockwise seen from its tip, face
    /// away from it when `to` lies further along it than `from`; a ring
    /// inside another, as `from`, faces back down the axis.
    fn band(&mut self, from: u32, to: u32, segments: u32) {
        for j in 0..segments {
            let (a, b) = (from + j, from + j + 1);
            let (d, c) = (to + j, to + j + 1);
            self.indices.extend([a, b, d, b, c, d]);
        }
    }

    /// A disc of triangles from `center` to a ring around an axis,
    /// counterclockwise seen from its tip: facing the tip when `forward`,
    /// away from it otherwise.
    fn fan(&mut self, center: u32, ring: u32, segments: u32, forward: bool) {
        for j in 0..segments {
            let (a, b) = (ring + j, ring + j + 1);
            if forward {
                self.indices.extend([center, a, b]);
            } else {
                self.indices.extend([center, b, a]);
            }
        }
    }

    fn finish(self, material: Material3D) -> TriangleMeshData {
        mesh(
            self.vertices,
            self.indices,
            self.normals,
            self.uvs,
            material,
        )
    }
}

/// The unit direction `j` of `segments` around an axis with the frame
/// `normal`, `binormal` (`binormal = axis × normal`): counterclockwise seen
/// from the axis' tip, the last one repeating the first exactly.
fn around(normal: DVec3, binormal: DVec3, j: u32, segments: u32) -> DVec3 {
    let angle = TAU * f64::from(j % segments) / f64::from(segments);
    normal * angle.cos() + binormal * angle.sin()
}

fn mesh(
    vertices: Vec<[f32; 3]>,
    indices: Vec<u32>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f32; 2]>,
    material: Material3D,
) -> TriangleMeshData {
    TriangleMeshData {
        vertices,
        indices,
        normals: Some(normals),
        uvs: Some(uvs),
        color: None,
        colors: None,
        material: Some(material),
    }
}

pub fn cube(size: f64, material: Material3D) -> Result<TriangleMeshData, Primitive3DError> {
    let h = dimension(size)? * 0.5;
    let faces = [
        (Vec3::X, -Vec3::Z, Vec3::Y),
        (-Vec3::X, Vec3::Z, Vec3::Y),
        (Vec3::Y, Vec3::X, -Vec3::Z),
        (-Vec3::Y, Vec3::X, Vec3::Z),
        (Vec3::Z, Vec3::X, Vec3::Y),
        (-Vec3::Z, -Vec3::X, Vec3::Y),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uvs = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, u, v) in faces {
        let center = normal * h;
        let base = vertices.len() as u32;
        for (point, uv) in [
            (center - u * h - v * h, Vec2::new(0.0, 0.0)),
            (center + u * h - v * h, Vec2::new(1.0, 0.0)),
            (center + u * h + v * h, Vec2::new(1.0, 1.0)),
            (center - u * h + v * h, Vec2::new(0.0, 1.0)),
        ] {
            vertices.push(point.to_array());
            normals.push(normal.to_array());
            uvs.push(uv.to_array());
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok(mesh(vertices, indices, normals, uvs, material))
}

pub fn plane(
    width: f64,
    height: f64,
    subdivisions: (u32, u32),
    material: Material3D,
) -> Result<TriangleMeshData, Primitive3DError> {
    let (width, height) = (dimension(width)?, dimension(height)?);
    let (sx, sz) = subdivisions;
    if sx == 0 || sz == 0 {
        return Err(Primitive3DError::InvalidSubdivisions);
    }
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for z in 0..=sz {
        let v = z as f32 / sz as f32;
        for x in 0..=sx {
            let u = x as f32 / sx as f32;
            vertices.push([(u - 0.5) * width, 0.0, (0.5 - v) * height]);
            normals.push(Vec3::Y.to_array());
            uvs.push([u, v]);
        }
    }
    let stride = sx + 1;
    let mut indices = Vec::new();
    for z in 0..sz {
        for x in 0..sx {
            let a = z * stride + x;
            let b = a + 1;
            let d = (z + 1) * stride + x;
            let c = d + 1;
            indices.extend_from_slice(&[a, b, c, a, c, d]);
        }
    }
    Ok(mesh(vertices, indices, normals, uvs, material))
}

pub fn sphere(
    radius: f64,
    segments: u32,
    rings: u32,
    material: Material3D,
) -> Result<TriangleMeshData, Primitive3DError> {
    let radius = dimension(radius)?;
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    if rings < 2 {
        return Err(Primitive3DError::InvalidRings);
    }
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for ring in 0..=rings {
        let v = ring as f32 / rings as f32;
        let phi = std::f32::consts::PI * v;
        for segment in 0..=segments {
            let u = segment as f32 / segments as f32;
            let theta = std::f32::consts::TAU * u;
            let normal = Vec3::new(phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
            vertices.push((normal * radius).to_array());
            normals.push(normal.to_array());
            uvs.push([u, v]);
        }
    }
    let stride = segments + 1;
    let mut indices = Vec::new();
    for ring in 0..rings {
        for segment in 0..segments {
            let a = ring * stride + segment;
            let d = a + 1;
            let b = (ring + 1) * stride + segment;
            let c = b + 1;
            indices.extend_from_slice(&[a, d, b, d, c, b]);
        }
    }
    Ok(mesh(vertices, indices, normals, uvs, material))
}

pub fn cylinder(
    radius: f64,
    height: f64,
    segments: u32,
    caps: bool,
    material: Material3D,
) -> Result<TriangleMeshData, Primitive3DError> {
    let (radius, half) = (dimension(radius)?, dimension(height)? * 0.5);
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let theta = std::f32::consts::TAU * u;
        let normal = Vec3::new(theta.cos(), 0.0, theta.sin());
        for (y, v) in [(-half, 0.0), (half, 1.0)] {
            vertices.push([normal.x * radius, y, normal.z * radius]);
            normals.push(normal.to_array());
            uvs.push([u, v]);
        }
    }
    for segment in 0..segments {
        let b0 = segment * 2;
        indices.extend_from_slice(&[b0, b0 + 1, b0 + 2, b0 + 2, b0 + 1, b0 + 3]);
    }
    if caps {
        add_cap(
            &mut vertices,
            &mut normals,
            &mut uvs,
            &mut indices,
            radius,
            half,
            segments,
            true,
        );
        add_cap(
            &mut vertices,
            &mut normals,
            &mut uvs,
            &mut indices,
            radius,
            -half,
            segments,
            false,
        );
    }
    Ok(mesh(vertices, indices, normals, uvs, material))
}

#[allow(clippy::too_many_arguments)]
fn add_cap(
    vertices: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
    radius: f32,
    y: f32,
    segments: u32,
    top: bool,
) {
    let center = vertices.len() as u32;
    let normal = if top { Vec3::Y } else { -Vec3::Y };
    vertices.push([0.0, y, 0.0]);
    normals.push(normal.to_array());
    uvs.push([0.5, 0.5]);
    for segment in 0..=segments {
        let theta = std::f32::consts::TAU * segment as f32 / segments as f32;
        let (x, z) = (theta.cos() * radius, theta.sin() * radius);
        vertices.push([x, y, z]);
        normals.push(normal.to_array());
        uvs.push([x / (2.0 * radius) + 0.5, z / (2.0 * radius) + 0.5]);
    }
    for segment in 0..segments {
        let a = center + 1 + segment;
        let b = a + 1;
        if top {
            indices.extend_from_slice(&[center, b, a]);
        } else {
            indices.extend_from_slice(&[center, a, b]);
        }
    }
}

pub fn cone(
    radius: f64,
    height: f64,
    segments: u32,
    cap: bool,
    material: Material3D,
) -> Result<TriangleMeshData, Primitive3DError> {
    let (radius, height) = (dimension(radius)?, dimension(height)?);
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    let half = height * 0.5;
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();
    for segment in 0..=segments {
        let u = segment as f32 / segments as f32;
        let theta = std::f32::consts::TAU * u;
        let normal = Vec3::new(theta.cos(), radius / height, theta.sin()).normalize();
        vertices.push([theta.cos() * radius, -half, theta.sin() * radius]);
        normals.push(normal.to_array());
        uvs.push([u, 0.0]);
        vertices.push([0.0, half, 0.0]);
        normals.push(normal.to_array());
        uvs.push([u, 1.0]);
    }
    for segment in 0..segments {
        let base = segment * 2;
        indices.extend_from_slice(&[base, base + 1, base + 2]);
    }
    if cap {
        add_cap(
            &mut vertices,
            &mut normals,
            &mut uvs,
            &mut indices,
            radius,
            -half,
            segments,
            false,
        );
    }
    Ok(mesh(vertices, indices, normals, uvs, material))
}

/// A Y-up ring torus: a tube `minor_radius` thick around a circle of
/// `major_radius` in the XZ plane, with `segments` around that circle and
/// `rings` around the tube.
pub fn torus(
    major_radius: f64,
    minor_radius: f64,
    segments: u32,
    rings: u32,
    material: Material3D,
) -> Result<TriangleMeshData, Primitive3DError> {
    let (major, minor) = (positive(major_radius)?, positive(minor_radius)?);
    if minor >= major {
        return Err(Primitive3DError::InvalidTorus);
    }
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    if rings < 3 {
        return Err(Primitive3DError::InvalidTorusRings);
    }
    let mut builder = RingBuilder::default();
    let mut previous = None;
    for i in 0..=segments {
        let u = f64::from(i) / f64::from(segments);
        // Each ring around the tube runs counterclockwise about the way the
        // circle goes on, so the bands between them face outward.
        let outward = around(DVec3::X, DVec3::Z, i, segments);
        let tangent = outward.cross(DVec3::Y);
        let ring = builder.ring(rings, |j| {
            let normal = around(outward, tangent.cross(outward), j, rings);
            (
                outward * major + normal * minor,
                normal,
                [u, f64::from(j) / f64::from(rings)],
            )
        });
        if let Some(previous) = previous {
            builder.band(previous, ring, rings);
        }
        previous = Some(ring);
    }
    Ok(builder.finish(material))
}

/// A closed, lit arrow from `start` to `end`: a capped cylinder shaft and a
/// cone head with `segments` sides. Built around the arrow's midpoint.
pub fn arrow_3d(
    start: DVec3,
    end: DVec3,
    sizes: ArrowSizes,
    segments: u32,
    material: Material3D,
) -> Result<PlacedMesh, Primitive3DError> {
    if !(start.is_finite() && end.is_finite()) {
        return Err(Primitive3DError::InvalidPoint);
    }
    let length = start.distance(end);
    if length <= 1e-9 {
        return Err(Primitive3DError::ZeroLengthArrow);
    }
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    let size = |value: Option<f64>, default: f64| value.map_or(Ok(default), positive);
    let shaft = size(sizes.shaft_radius, 0.025 * length)?;
    let head_radius = size(sizes.head_radius, 3.0 * shaft)?;
    let head_length = size(sizes.head_length, 0.22 * length)?;
    if head_length >= length || head_radius <= shaft {
        return Err(Primitive3DError::InvalidArrowHead);
    }
    let axis = (end - start) / length;
    let normal = axis.any_orthonormal_vector();
    let binormal = axis.cross(normal);
    let center = (start + end) / 2.0;
    let tail = start - center;
    let neck = tail + axis * (length - head_length);
    let tip = end - center;
    let side = |j: u32| around(normal, binormal, j, segments);
    let u = |j: u32| f64::from(j) / f64::from(segments);
    let disc = |direction: DVec3| {
        [
            0.5 + 0.5 * direction.dot(normal),
            0.5 + 0.5 * direction.dot(binormal),
        ]
    };
    let v_neck = (length - head_length) / length;
    let mut builder = RingBuilder::default();

    // The tail's cap, facing back.
    let cap = builder.vertex(tail, -axis, [0.5, 0.5]);
    let cap_ring = builder.ring(segments, |j| (tail + side(j) * shaft, -axis, disc(side(j))));
    builder.fan(cap, cap_ring, segments, false);
    // The shaft.
    let shaft_tail = builder.ring(segments, |j| (tail + side(j) * shaft, side(j), [u(j), 0.0]));
    let shaft_neck = builder.ring(segments, |j| {
        (neck + side(j) * shaft, side(j), [u(j), v_neck])
    });
    builder.band(shaft_tail, shaft_neck, segments);
    // The head's base, an annulus around the shaft facing back.
    let inner = builder.ring(segments, |j| {
        (
            neck + side(j) * shaft,
            -axis,
            disc(side(j) * shaft / head_radius),
        )
    });
    let outer = builder.ring(segments, |j| {
        (neck + side(j) * head_radius, -axis, disc(side(j)))
    });
    builder.band(inner, outer, segments);
    // The cone, smooth: one tip vertex per side, with the normal halfway
    // between the side's edges.
    let slope = |direction: DVec3| direction * head_length + axis * head_radius;
    let base = builder.ring(segments, |j| {
        (neck + side(j) * head_radius, slope(side(j)), [u(j), v_neck])
    });
    for j in 0..segments {
        let middle = (side(j) + side(j + 1)).normalize_or(side(j));
        let apex = builder.vertex(tip, slope(middle), [(u(j) + u(j + 1)) / 2.0, 1.0]);
        builder.indices.extend([base + j, base + j + 1, apex]);
    }
    Ok(PlacedMesh {
        mesh: builder.finish(material),
        center,
    })
}

/// A closed, lit tube `radius` thick along the polyline through `points`,
/// with `segments` sides, built around the center of the points' box.
///
/// The cross-sections follow the path with rotation-minimizing (parallel
/// transport) frames, so the tube does not twist; a `closed` loop spreads
/// the turn its frames gather on the way round evenly along it, so its seam
/// meets. At a corner the cross-section lies in the plane halfway between
/// both segments, stretched to keep the radius (up to three radii), and a
/// turn sharper than 35° creases. `caps` closes the ends of an open tube.
/// Repeated points are skipped, as is a last point repeating the first of a
/// closed loop.
pub fn tube(
    points: &[DVec3],
    radius: f64,
    segments: u32,
    closed: bool,
    caps: bool,
    material: Material3D,
) -> Result<PlacedMesh, Primitive3DError> {
    let radius = positive(radius)?;
    if segments < 3 {
        return Err(Primitive3DError::InvalidSegments);
    }
    if !points.iter().all(|point| point.is_finite()) {
        return Err(Primitive3DError::InvalidPoint);
    }
    let (min, max) = points.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(min, max), point| (min.min(*point), max.max(*point)),
    );
    let epsilon = (max - min).max_element().max(1.0) * 1e-9;
    let mut path: Vec<DVec3> = Vec::with_capacity(points.len());
    for point in points {
        if path
            .last()
            .is_none_or(|last| last.distance(*point) > epsilon)
        {
            path.push(*point);
        }
    }
    if closed && path.len() > 1 && path[0].distance(path[path.len() - 1]) <= epsilon {
        path.pop();
    }
    if path.len() < if closed { 3 } else { 2 } {
        return Err(Primitive3DError::TooFewPoints);
    }
    let center = (min + max) / 2.0;
    let path: Vec<DVec3> = path.into_iter().map(|point| point - center).collect();
    let count = path.len();
    let edges = if closed { count } else { count - 1 };
    let directions: Vec<DVec3> = (0..edges)
        .map(|k| (path[(k + 1) % count] - path[k]).normalize())
        .collect();
    // The directions a point is reached by and left along; an open tube's
    // ends continue their segment.
    let incoming = |i: usize| match (closed, i) {
        (true, _) => directions[(i + edges - 1) % edges],
        (false, 0) => directions[0],
        (false, _) => directions[i - 1],
    };
    let outgoing = |i: usize| directions[i.min(edges - 1)];
    let tangents: Vec<DVec3> = (0..count)
        .map(|i| {
            (incoming(i) + outgoing(i))
                .try_normalize()
                .unwrap_or(incoming(i))
        })
        .collect();
    // Distance along the path to each point, and its whole length.
    let mut along = Vec::with_capacity(count);
    let mut total = 0.0;
    for i in 0..count {
        along.push(total);
        total += path[i].distance(path[(i + 1) % count]);
    }
    if !closed {
        total = along[count - 1];
    }
    // `normal` turned the least way from `from` to `to`, kept unit and
    // perpendicular to `to`.
    let transport = |normal: DVec3, from: DVec3, to: DVec3| {
        let moved = DQuat::from_rotation_arc(from, to) * normal;
        (moved - to * moved.dot(to))
            .try_normalize()
            .unwrap_or_else(|| to.any_orthonormal_vector())
    };
    // One frame per segment, each carried over from the one before: the
    // walls of a segment are straight.
    let mut frames = Vec::with_capacity(edges);
    frames.push(directions[0].any_orthonormal_vector());
    for k in 1..edges {
        frames.push(transport(frames[k - 1], directions[k - 1], directions[k]));
    }
    // Back at the start a loop's frame has turned about the tangent; every
    // cross-section turns back by its share of the way, so the loop meets
    // itself.
    let twist = if closed {
        let back = transport(frames[edges - 1], directions[edges - 1], directions[0]);
        back.cross(frames[0])
            .dot(directions[0])
            .atan2(back.dot(frames[0]))
    } else {
        0.0
    };
    // Each cross-section's frame: its outgoing segment's, tilted into the
    // plane halfway to the incoming one (where both segments' frames meet).
    let normals: Vec<DVec3> = (0..count)
        .map(|i| {
            let segment = i.min(edges - 1);
            let normal = transport(frames[segment], directions[segment], tangents[i]);
            DQuat::from_axis_angle(tangents[i], twist * along[i] / total) * normal
        })
        .collect();

    let mut builder = RingBuilder::default();
    let v = |j: u32| f64::from(j) / f64::from(segments);
    // Where vertex `j` of point `i`'s cross-section sits, from the point.
    let offset = |i: usize, j: u32| {
        let (tangent, normal) = (tangents[i], normals[i]);
        let direction = around(normal, tangent.cross(normal), j, segments);
        let (from, to) = (incoming(i), outgoing(i));
        let miter = (1.0 / tangent.dot(from).max(1e-6)).min(TUBE_MAX_MITER);
        match (to - from).try_normalize() {
            Some(bend) => (direction + bend * (direction.dot(bend) * (miter - 1.0))) * radius,
            None => direction * radius,
        }
    };
    // Facing away from the axis of the segment along `direction`.
    let radial = |offset: DVec3, direction: DVec3| offset - direction * offset.dot(direction);
    let ring = |builder: &mut RingBuilder, i: usize, u: f64, side: Option<DVec3>| {
        builder.ring(segments, |j| {
            let offset = offset(i, j);
            let normal = match side {
                Some(direction) => radial(offset, direction),
                None => {
                    radial(offset, incoming(i)).normalize_or_zero()
                        + radial(offset, outgoing(i)).normalize_or_zero()
                }
            };
            (path[i] + offset, normal, [u, v(j)])
        })
    };
    // The rings each segment starts and ends at: one per point, or two at a
    // crease, each with the normals of its own segment.
    let mut starts = Vec::with_capacity(count);
    let mut ends = Vec::with_capacity(count);
    for (i, distance) in along.iter().enumerate() {
        let u = distance / total;
        if incoming(i).dot(outgoing(i)) >= TUBE_SMOOTH_ANGLE.cos() {
            let shared = ring(&mut builder, i, u, None);
            ends.push(shared);
            starts.push(shared);
        } else {
            ends.push(ring(&mut builder, i, u, Some(incoming(i))));
            starts.push(ring(&mut builder, i, u, Some(outgoing(i))));
        }
    }
    for k in 0..edges {
        let end = if k + 1 < count {
            ends[k + 1]
        } else {
            // The loop's last segment ends on a copy of the first ring at
            // the far end of the texture.
            let side =
                (incoming(0).dot(outgoing(0)) < TUBE_SMOOTH_ANGLE.cos()).then(|| incoming(0));
            ring(&mut builder, 0, 1.0, side)
        };
        builder.band(starts[k], end, segments);
    }
    if !closed && caps {
        for (i, forward) in [(0, false), (count - 1, true)] {
            let facing = if forward { tangents[i] } else { -tangents[i] };
            let cap = builder.vertex(path[i], facing, [0.5, 0.5]);
            let rim = builder.ring(segments, |j| {
                let offset = offset(i, j);
                let disc = [
                    0.5 + 0.5 * offset.dot(normals[i]) / radius,
                    0.5 + 0.5 * offset.dot(tangents[i].cross(normals[i])) / radius,
                ];
                (path[i] + offset, facing, disc)
            });
            builder.fan(cap, rim, segments, forward);
        }
    }
    Ok(PlacedMesh {
        mesh: builder.finish(material),
        center,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn assert_valid(mesh: &TriangleMeshData) {
        assert_eq!(mesh.normals.as_ref().unwrap().len(), mesh.vertices.len());
        assert_eq!(mesh.uvs.as_ref().unwrap().len(), mesh.vertices.len());
        assert!(
            mesh.indices
                .iter()
                .all(|index| *index < mesh.vertices.len() as u32)
        );
        assert!(
            mesh.normals
                .as_ref()
                .unwrap()
                .iter()
                .all(|normal| { (Vec3::from_array(*normal).length() - 1.0).abs() < 1e-5 })
        );
        assert!(
            mesh.uvs
                .as_ref()
                .unwrap()
                .iter()
                .flatten()
                .all(|uv| (0.0..=1.0).contains(uv))
        );
        for triangle in mesh.indices.as_chunks::<3>().0 {
            let a = Vec3::from_array(mesh.vertices[triangle[0] as usize]);
            let b = Vec3::from_array(mesh.vertices[triangle[1] as usize]);
            let c = Vec3::from_array(mesh.vertices[triangle[2] as usize]);
            let face = (b - a).cross(c - a);
            if face.length_squared() < 1e-10 {
                continue;
            }
            let normals = mesh.normals.as_ref().unwrap();
            let expected = (Vec3::from_array(normals[triangle[0] as usize])
                + Vec3::from_array(normals[triangle[1] as usize])
                + Vec3::from_array(normals[triangle[2] as usize]))
            .normalize_or_zero();
            assert!(face.normalize().dot(expected) > 0.5);
        }
    }

    #[test]
    fn primitives_have_complete_vertex_attributes() {
        let material = Material3D::default();
        for mesh in [
            cube(2.0, material).unwrap(),
            plane(2.0, 3.0, (2, 3), material).unwrap(),
            sphere(1.0, 12, 6, material).unwrap(),
            cylinder(1.0, 2.0, 12, true, material).unwrap(),
            cone(1.0, 2.0, 12, true, material).unwrap(),
        ] {
            assert_valid(&mesh);
        }
    }

    #[test]
    fn cube_has_per_face_geometry_and_unit_bounds() {
        let mesh = cube(2.0, Material3D::default()).unwrap();
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
        assert!(
            mesh.vertices
                .iter()
                .flatten()
                .all(|v| (-1.0..=1.0).contains(v))
        );
    }

    #[test]
    fn y_up_dimensions_and_topology_are_stable() {
        let material = Material3D::default();
        let plane = plane(4.0, 6.0, (2, 3), material).unwrap();
        assert_eq!(plane.vertices.len(), 12);
        assert_eq!(plane.indices.len(), 36);
        assert!(plane.vertices.iter().all(|vertex| vertex[1] == 0.0));

        let cylinder = cylinder(2.0, 6.0, 8, true, material).unwrap();
        assert_eq!(cylinder.indices.len(), 8 * 12);
        assert_eq!(
            cylinder
                .vertices
                .iter()
                .map(|v| v[1])
                .fold(f32::INFINITY, f32::min),
            -3.0
        );
        assert_eq!(
            cylinder
                .vertices
                .iter()
                .map(|v| v[1])
                .fold(f32::NEG_INFINITY, f32::max),
            3.0
        );

        let cone = cone(2.0, 6.0, 8, true, material).unwrap();
        assert_eq!(cone.indices.len(), 8 * 6);
        assert_eq!(
            cone.vertices
                .iter()
                .map(|v| v[1])
                .fold(f32::INFINITY, f32::min),
            -3.0
        );
        assert_eq!(
            cone.vertices
                .iter()
                .map(|v| v[1])
                .fold(f32::NEG_INFINITY, f32::max),
            3.0
        );
    }

    #[test]
    fn rejects_invalid_parameters() {
        let material = Material3D::default();
        assert!(matches!(
            cube(0.0, material),
            Err(Primitive3DError::InvalidDimension)
        ));
        assert!(matches!(
            cube(f64::NAN, material),
            Err(Primitive3DError::InvalidDimension)
        ));
        assert!(matches!(
            cylinder(1.0, f64::INFINITY, 8, true, material),
            Err(Primitive3DError::InvalidDimension)
        ));
        assert!(matches!(
            sphere(1.0, 2, 8, material),
            Err(Primitive3DError::InvalidSegments)
        ));
        assert!(matches!(
            sphere(1.0, 8, 1, material),
            Err(Primitive3DError::InvalidRings)
        ));
        assert!(matches!(
            plane(1.0, 1.0, (0, 1), material),
            Err(Primitive3DError::InvalidSubdivisions)
        ));
    }

    /// Every edge, by position, is shared by exactly two triangles in
    /// opposite directions: the surface is closed and consistently wound.
    fn assert_closed(mesh: &TriangleMeshData) {
        let key = |i: u32| mesh.vertices[i as usize].map(|c| (c * 1e4).round() as i64);
        let mut edges: HashMap<([i64; 3], [i64; 3]), i32> = HashMap::new();
        for triangle in mesh.indices.as_chunks::<3>().0 {
            for k in 0..3 {
                let (a, b) = (key(triangle[k]), key(triangle[(k + 1) % 3]));
                if a != b {
                    *edges.entry((a, b)).or_default() += 1;
                }
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

    /// The volume a closed mesh encloses: positive when it is wound
    /// outward.
    fn signed_volume(mesh: &TriangleMeshData) -> f32 {
        mesh.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| {
                let [a, b, c] = [0, 1, 2].map(|k| Vec3::from_array(mesh.vertices[t[k] as usize]));
                a.dot(b.cross(c)) / 6.0
            })
            .sum()
    }

    fn distance_to_line(point: Vec3, from: Vec3, to: Vec3) -> f32 {
        let direction = (to - from).normalize();
        let offset = point - from;
        (offset - direction * offset.dot(direction)).length()
    }

    #[test]
    fn a_torus_is_closed_smooth_and_wound_outward() {
        let mesh = torus(2.0, 0.5, 64, 32, Material3D::default()).unwrap();
        assert_valid(&mesh);
        assert_closed(&mesh);
        let volume = 2.0 * std::f32::consts::PI.powi(2) * 2.0 * 0.5f32.powi(2);
        assert!((signed_volume(&mesh) / volume - 1.0).abs() < 0.02);
        // Every vertex lies on the tube, its normal pointing away from the
        // circle at its core.
        for (vertex, normal) in mesh.vertices.iter().zip(mesh.normals.as_ref().unwrap()) {
            let vertex = Vec3::from_array(*vertex);
            let core = Vec3::new(vertex.x, 0.0, vertex.z).normalize() * 2.0;
            assert!(((vertex - core).length() - 0.5).abs() < 1e-5);
            assert!((vertex - core).normalize().dot(Vec3::from_array(*normal)) > 0.999);
        }
    }

    #[test]
    fn an_arrow_spans_its_points_around_its_midpoint() {
        let (start, end) = (DVec3::new(1.0, 1.0, 0.0), DVec3::new(1.0, 1.0, 4.0));
        let arrow = arrow_3d(start, end, ArrowSizes::default(), 64, Material3D::default()).unwrap();
        assert_eq!(arrow.center, DVec3::new(1.0, 1.0, 2.0));
        let mesh = &arrow.mesh;
        assert_valid(mesh);
        assert_closed(mesh);
        let zs = mesh.vertices.iter().map(|v| v[2]);
        assert_eq!(zs.clone().fold(f32::INFINITY, f32::min), -2.0);
        assert_eq!(zs.fold(f32::NEG_INFINITY, f32::max), 2.0);
        // Default sizes follow the length: a shaft 0.1 and a head 0.3 wide
        // at its base and 0.88 long.
        let (shaft, head, head_length) = (0.1f32, 0.3f32, 0.88f32);
        let width = mesh
            .vertices
            .iter()
            .map(|v| v[0].hypot(v[1]))
            .fold(0.0, f32::max);
        assert!((width - head).abs() < 1e-5);
        let pi = std::f32::consts::PI;
        let volume =
            pi * shaft * shaft * (4.0 - head_length) + pi * head * head * head_length / 3.0;
        assert!((signed_volume(mesh) / volume - 1.0).abs() < 0.01);

        let sizes = ArrowSizes {
            shaft_radius: Some(0.2),
            head_radius: None,
            head_length: Some(1.0),
        };
        let thick = arrow_3d(
            DVec3::ZERO,
            DVec3::new(-3.0, 2.0, 1.0),
            sizes,
            12,
            Material3D::default(),
        )
        .unwrap()
        .mesh;
        assert_valid(&thick);
        assert_closed(&thick);
        assert!(signed_volume(&thick) > 0.0);
    }

    #[test]
    fn tubes_close_with_caps_or_as_loops() {
        let material = Material3D::default();
        let path = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(2.0, 2.0, 0.0),
            DVec3::new(2.0, 2.0, 2.0),
        ];
        let capped = tube(&path, 0.2, 12, false, true, material).unwrap();
        assert_eq!(capped.center, DVec3::new(1.0, 1.0, 1.0));
        assert_valid(&capped.mesh);
        assert_closed(&capped.mesh);
        // Three straight runs of 2 mitered at the corners hold as much as
        // three 12-sided prisms that long.
        let area = 0.5 * 12.0 * 0.04 * (std::f32::consts::TAU / 12.0).sin();
        assert!((signed_volume(&capped.mesh) / (6.0 * area) - 1.0).abs() < 1e-3);

        let loop_points: Vec<DVec3> = (0..40)
            .map(|k| {
                let t = TAU * f64::from(k) / 40.0;
                // A trefoil knot: its frames turn on the way round.
                DVec3::new(
                    t.sin() + 2.0 * (2.0 * t).sin(),
                    t.cos() - 2.0 * (2.0 * t).cos(),
                    -(3.0 * t).sin(),
                )
            })
            .collect();
        let knot = tube(&loop_points, 0.3, 10, true, true, material).unwrap();
        assert_valid(&knot.mesh);
        assert_closed(&knot.mesh);
        assert!(signed_volume(&knot.mesh) > 0.0);

        // Without caps an open tube stays open.
        let open = tube(&path, 0.2, 12, false, false, material).unwrap().mesh;
        assert_valid(&open);
        let mut edges = HashMap::new();
        for t in open.indices.as_chunks::<3>().0 {
            for k in 0..3 {
                let key = |i: u32| open.vertices[i as usize].map(|c| (c * 1e4).round() as i64);
                *edges.entry((key(t[k]), key(t[(k + 1) % 3]))).or_insert(0) += 1;
            }
        }
        assert!(edges.keys().any(|(a, b)| !edges.contains_key(&(*b, *a))));
    }

    #[test]
    fn tube_corners_keep_the_radius() {
        let path = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(2.0, 2.0, 0.0),
        ];
        let mesh = tube(&path, 0.25, 16, false, false, Material3D::default())
            .unwrap()
            .mesh;
        let points = path.map(|p| (p - DVec3::new(1.0, 1.0, 0.0)).as_vec3());
        for vertex in &mesh.vertices {
            let vertex = Vec3::from_array(*vertex);
            let nearest = (0..2)
                .map(|k| (distance_to_line(vertex, points[k], points[k + 1]) - 0.25).abs())
                .fold(f32::INFINITY, f32::min);
            assert!(nearest < 1e-5, "{vertex:?}");
        }
        // The corner creases: its two rings share positions, not normals.
        assert_eq!(mesh.vertices.len(), 4 * 17);
    }

    #[test]
    fn tube_frames_do_not_twist() {
        // A planar circle: parallel transport keeps every frame's first
        // direction at the same height over the plane.
        let points: Vec<DVec3> = (0..32)
            .map(|k| {
                let t = TAU * f64::from(k) / 32.0;
                DVec3::new(2.0 * t.cos(), 0.0, 2.0 * t.sin())
            })
            .collect();
        let mesh = tube(&points, 0.1, 8, true, false, Material3D::default())
            .unwrap()
            .mesh;
        assert_closed(&mesh);
        let heights: Vec<f32> = mesh.vertices.chunks(9).map(|ring| ring[0][1]).collect();
        assert_eq!(heights.len(), 33);
        assert!(heights.iter().all(|h| (h - heights[0]).abs() < 1e-5));

        // Repeated points and a last point repeating the first are skipped.
        let mut repeated = points.clone();
        repeated.insert(3, points[3]);
        repeated.push(points[0]);
        let same = tube(&repeated, 0.1, 8, true, false, Material3D::default())
            .unwrap()
            .mesh;
        assert_eq!(same.vertices, mesh.vertices);
    }

    #[test]
    fn rejects_invalid_new_primitives() {
        let material = Material3D::default();
        assert!(matches!(
            torus(1.0, 1.0, 8, 8, material),
            Err(Primitive3DError::InvalidTorus)
        ));
        assert!(matches!(
            torus(1.0, 0.2, 8, 2, material),
            Err(Primitive3DError::InvalidTorusRings)
        ));
        assert!(matches!(
            torus(1.0, 0.2, 2, 8, material),
            Err(Primitive3DError::InvalidSegments)
        ));
        let sizes = ArrowSizes::default();
        assert!(matches!(
            arrow_3d(DVec3::ONE, DVec3::ONE, sizes, 8, material),
            Err(Primitive3DError::ZeroLengthArrow)
        ));
        assert!(matches!(
            arrow_3d(DVec3::ZERO, DVec3::X * f64::NAN, sizes, 8, material),
            Err(Primitive3DError::InvalidPoint)
        ));
        for sizes in [
            ArrowSizes {
                head_length: Some(1.0),
                ..sizes
            },
            ArrowSizes {
                shaft_radius: Some(0.2),
                head_radius: Some(0.2),
                ..sizes
            },
        ] {
            assert!(matches!(
                arrow_3d(DVec3::ZERO, DVec3::X, sizes, 8, material),
                Err(Primitive3DError::InvalidArrowHead)
            ));
        }
        assert!(matches!(
            arrow_3d(
                DVec3::ZERO,
                DVec3::X,
                ArrowSizes {
                    shaft_radius: Some(-0.1),
                    ..sizes
                },
                8,
                material
            ),
            Err(Primitive3DError::InvalidDimension)
        ));
        let line = [DVec3::ZERO, DVec3::X];
        assert!(matches!(
            tube(&line[..1], 0.1, 8, false, true, material),
            Err(Primitive3DError::TooFewPoints)
        ));
        assert!(matches!(
            tube(&[DVec3::ZERO, DVec3::ZERO], 0.1, 8, false, true, material),
            Err(Primitive3DError::TooFewPoints)
        ));
        assert!(matches!(
            tube(&line, 0.1, 8, true, true, material),
            Err(Primitive3DError::TooFewPoints)
        ));
        assert!(matches!(
            tube(
                &[DVec3::ZERO, DVec3::INFINITY],
                0.1,
                8,
                false,
                true,
                material
            ),
            Err(Primitive3DError::InvalidPoint)
        ));
        assert!(matches!(
            tube(&line, 0.0, 8, false, true, material),
            Err(Primitive3DError::InvalidDimension)
        ));
    }
}
