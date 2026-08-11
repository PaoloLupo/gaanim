//! Native deterministic geometry for friendly Y-up 3D primitives.

use gaanim_core::glam::{Vec2, Vec3};
use gaanim_scene::{Material3D, TriangleMeshData};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Primitive3DError {
    #[error("3D primitive dimensions must be finite and greater than zero")]
    InvalidDimension,
    #[error("3D primitive segments must be at least 3")]
    InvalidSegments,
    #[error("sphere rings must be at least 2")]
    InvalidRings,
    #[error("plane subdivisions must both be at least 1")]
    InvalidSubdivisions,
}

fn dimension(value: f64) -> Result<f32, Primitive3DError> {
    (value.is_finite() && value > 0.0)
        .then_some(value as f32)
        .ok_or(Primitive3DError::InvalidDimension)
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

#[cfg(test)]
mod tests {
    use super::*;

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
        for triangle in mesh.indices.chunks_exact(3) {
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
}
