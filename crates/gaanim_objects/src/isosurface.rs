//! Isosurfaces of scalar fields sampled on a grid, by surface nets: one
//! vertex in each cell the surface crosses, at the mean of the points where
//! it crosses the cell's edges, and one quad across each grid edge it
//! crosses, joining the vertices of the four cells around that edge. The
//! surface faces out, toward the side of the level the walls of the box
//! mostly lie on, and its normals follow the field's gradient, so it shades
//! smoothly at any grid.

use gaanim_scene::{Material3D, TriangleMeshData};
use glam::DVec3;

/// A scalar field sampled at `size[0] * size[1] * size[2]` points that
/// span the box from `min` to `max`, x fastest.
#[derive(Debug, Clone, PartialEq)]
pub struct SampledField {
    pub values: Vec<f32>,
    pub size: [usize; 3],
    pub min: DVec3,
    pub max: DVec3,
}

#[derive(Debug, Clone, Copy, PartialEq, thiserror::Error)]
pub enum IsosurfaceError {
    #[error(
        "a sampled field needs at least 2 points on each side, one value per point and a box with min < max"
    )]
    InvalidField,
    #[error("the level must be finite")]
    InvalidLevel,
    #[error("the field does not cross the level {0} anywhere in its box")]
    NoSurface(f32),
}

/// The surface where a field takes one value: its vertices and their
/// normals, its triangles, and whether the box cuts it open.
#[derive(Debug, Clone, PartialEq)]
pub struct Isosurface {
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<u32>,
    pub open: bool,
}

impl SampledField {
    fn validate(&self) -> Result<(), IsosurfaceError> {
        let [nx, ny, nz] = self.size;
        let points = nx.checked_mul(ny).and_then(|count| count.checked_mul(nz));
        let valid = nx >= 2
            && ny >= 2
            && nz >= 2
            && points == Some(self.values.len())
            && self.min.is_finite()
            && self.max.is_finite()
            && self.min.cmplt(self.max).all();
        valid.then_some(()).ok_or(IsosurfaceError::InvalidField)
    }

    fn index(&self, [x, y, z]: [usize; 3]) -> usize {
        x + self.size[0] * (y + self.size[1] * z)
    }

    fn at(&self, point: [usize; 3]) -> f32 {
        self.values[self.index(point)]
    }

    /// The field's gradient at a grid point, per grid step, by central
    /// differences (one-sided at the edges of the grid).
    fn gradient(&self, point: [usize; 3]) -> DVec3 {
        let mut gradient = [0.0; 3];
        for (axis, slot) in gradient.iter_mut().enumerate() {
            let mut before = point;
            let mut after = point;
            before[axis] = point[axis].saturating_sub(1);
            after[axis] = (point[axis] + 1).min(self.size[axis] - 1);
            let span = (after[axis] - before[axis]) as f64;
            *slot = f64::from(self.at(after) - self.at(before)) / span;
        }
        DVec3::from_array(gradient)
    }

    /// Where a point in grid steps lies in the box.
    fn place(&self, grid: DVec3) -> DVec3 {
        let steps = DVec3::new(
            (self.size[0] - 1) as f64,
            (self.size[1] - 1) as f64,
            (self.size[2] - 1) as f64,
        );
        self.min + (self.max - self.min) * grid / steps
    }
}

/// The corners of a cell, as offsets from its lowest corner, by the bits
/// of their index (x, y, z).
const CORNERS: [[usize; 3]; 8] = [
    [0, 0, 0],
    [1, 0, 0],
    [0, 1, 0],
    [1, 1, 0],
    [0, 0, 1],
    [1, 0, 1],
    [0, 1, 1],
    [1, 1, 1],
];

/// The twelve edges of a cell, as pairs of its corners.
const EDGES: [[usize; 2]; 12] = [
    [0, 1],
    [2, 3],
    [4, 5],
    [6, 7],
    [0, 2],
    [1, 3],
    [4, 6],
    [5, 7],
    [0, 4],
    [1, 5],
    [2, 6],
    [3, 7],
];

/// The surface where `field` equals `level`.
pub fn isosurface(field: &SampledField, level: f32) -> Result<Isosurface, IsosurfaceError> {
    field.validate()?;
    if !level.is_finite() {
        return Err(IsosurfaceError::InvalidLevel);
    }
    let [nx, ny, nz] = field.size;
    let cells = [nx - 1, ny - 1, nz - 1];
    // A non-finite value counts as below the level: a hole closes there.
    let above = |point: [usize; 3]| field.at(point) > level;
    let mut vertex_of = vec![u32::MAX; cells[0] * cells[1] * cells[2]];
    let cell_index = |[x, y, z]: [usize; 3]| x + cells[0] * (y + cells[1] * z);
    let mut vertices = Vec::new();
    let mut normals = Vec::new();
    for z in 0..cells[2] {
        for y in 0..cells[1] {
            for x in 0..cells[0] {
                let corner = |offset: [usize; 3]| [x + offset[0], y + offset[1], z + offset[2]];
                let inside = CORNERS.map(|offset| above(corner(offset)));
                if inside.iter().all(|&inside| inside) || inside.iter().all(|&inside| !inside) {
                    continue;
                }
                // The mean of the points where the surface crosses the
                // cell's edges, in steps from its lowest corner.
                let mut sum = DVec3::ZERO;
                let mut crossings = 0.0;
                for [a, b] in EDGES {
                    if inside[a] == inside[b] {
                        continue;
                    }
                    let (va, vb) = (field.at(corner(CORNERS[a])), field.at(corner(CORNERS[b])));
                    let t = if va.is_finite() && vb.is_finite() && va != vb {
                        f64::from((level - va) / (vb - va)).clamp(0.0, 1.0)
                    } else {
                        0.5
                    };
                    let from = DVec3::from_array(CORNERS[a].map(|step| step as f64));
                    let to = DVec3::from_array(CORNERS[b].map(|step| step as f64));
                    sum += from + (to - from) * t;
                    crossings += 1.0;
                }
                let local = sum / crossings;
                // The gradient there, blended from the cell's corners.
                let mut gradient = DVec3::ZERO;
                for (offset, step) in CORNERS
                    .iter()
                    .map(|offset| (offset, DVec3::from_array(offset.map(|step| step as f64))))
                {
                    let weight = (DVec3::ONE - step - local + 2.0 * step * local).element_product();
                    let grown = field.gradient(corner(*offset));
                    if grown.is_finite() {
                        gradient += grown * weight;
                    }
                }
                let lowest = DVec3::new(x as f64, y as f64, z as f64);
                let position = field.place(lowest + local);
                // Per scene unit rather than per grid step.
                let steps = (field.max - field.min)
                    / DVec3::new(cells[0] as f64, cells[1] as f64, cells[2] as f64);
                let normal = (gradient / steps).normalize_or_zero();
                vertex_of[cell_index([x, y, z])] = vertices.len() as u32;
                vertices.push(position.as_vec3().to_array());
                normals.push(normal.as_vec3().to_array());
            }
        }
    }
    // A quad across each grid edge the surface crosses, facing the end
    // above the level.
    let mut indices = Vec::new();
    // An edge on a side of the box has cells on one side only: the box cuts
    // the surface open there.
    let mut open = false;
    for axis in 0..3 {
        let (b, c) = ((axis + 1) % 3, (axis + 2) % 3);
        for z in 0..nz {
            for y in 0..ny {
                for x in 0..nx {
                    let start = [x, y, z];
                    if start[axis] + 1 >= field.size[axis] {
                        continue;
                    }
                    let mut end = start;
                    end[axis] += 1;
                    let (from, to) = (above(start), above(end));
                    if from == to {
                        continue;
                    }
                    // The four cells around the edge, counterclockwise seen
                    // from the end of the axis.
                    if start[b] == 0
                        || start[c] == 0
                        || start[b] >= field.size[b] - 1
                        || start[c] >= field.size[c] - 1
                    {
                        open = true;
                        continue;
                    }
                    let cell = |db: usize, dc: usize| {
                        let mut index = start;
                        index[b] = start[b] - 1 + db;
                        index[c] = start[c] - 1 + dc;
                        vertex_of[cell_index(index)]
                    };
                    let quad = [cell(0, 0), cell(1, 0), cell(1, 1), cell(0, 1)];
                    if quad.contains(&u32::MAX) {
                        continue;
                    }
                    let [p, q, r, s] = if to {
                        quad
                    } else {
                        [quad[0], quad[3], quad[2], quad[1]]
                    };
                    indices.extend_from_slice(&[p, q, r, p, r, s]);
                }
            }
        }
    }
    if indices.is_empty() {
        return Err(IsosurfaceError::NoSurface(level));
    }
    // Facing where the field grows, the surface of a field that grows
    // inward, a density's, faces in: it faces out instead when most of the
    // box's walls lie below the level.
    let mut walls = 0_usize;
    let mut above_walls = 0_usize;
    for z in 0..nz {
        for y in 0..ny {
            for x in 0..nx {
                let point = [x, y, z];
                let on_wall = point
                    .iter()
                    .zip(field.size)
                    .any(|(&at, count)| at == 0 || at + 1 == count);
                if on_wall {
                    walls += 1;
                    above_walls += usize::from(above(point));
                }
            }
        }
    }
    if above_walls * 2 < walls {
        for triangle in indices.chunks_mut(3) {
            triangle.swap(1, 2);
        }
        for normal in &mut normals {
            *normal = normal.map(|value| -value);
        }
    }
    Ok(Isosurface {
        vertices,
        normals,
        indices,
        open,
    })
}

impl Isosurface {
    /// The surface as a lit mesh in `material`: drawn on both sides when
    /// the box cuts it open, since a lit mesh hides the faces turned away
    /// and an open surface shows its inside.
    pub fn mesh(self, material: Material3D) -> TriangleMeshData {
        let Self {
            mut vertices,
            mut normals,
            mut indices,
            open,
        } = self;
        if open {
            let count = vertices.len() as u32;
            vertices.extend_from_within(..);
            normals.extend_from_within(..);
            for normal in &mut normals[count as usize..] {
                *normal = normal.map(|value| -value);
            }
            let back: Vec<u32> = indices
                .chunks(3)
                .flat_map(|triangle| {
                    [
                        triangle[0] + count,
                        triangle[2] + count,
                        triangle[1] + count,
                    ]
                })
                .collect();
            indices.extend(back);
        }
        TriangleMeshData {
            vertices,
            indices,
            normals: Some(normals),
            uvs: None,
            color: None,
            colors: None,
            material: Some(material),
            surface: None,
            blend: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `function` sampled on `n` points a side over the box from -2 to 2.
    fn sampled(n: usize, function: impl Fn(DVec3) -> f64) -> SampledField {
        let min = DVec3::splat(-2.0);
        let max = DVec3::splat(2.0);
        let mut values = Vec::with_capacity(n * n * n);
        for z in 0..n {
            for y in 0..n {
                for x in 0..n {
                    let at = DVec3::new(x as f64, y as f64, z as f64) / (n - 1) as f64;
                    values.push(function(min + (max - min) * at) as f32);
                }
            }
        }
        SampledField {
            values,
            size: [n; 3],
            min,
            max,
        }
    }

    /// A sphere's isosurface lies on the sphere, faces out and is closed.
    #[test]
    fn a_sphere_is_closed_faces_out_and_lies_at_its_radius() {
        let field = sampled(32, |point| point.length_squared());
        let surface = isosurface(&field, 1.0).unwrap();
        assert!(!surface.open);
        assert!(surface.vertices.len() > 500);
        for (vertex, normal) in surface.vertices.iter().zip(&surface.normals) {
            let (vertex, normal) = (glam::Vec3::from(*vertex), glam::Vec3::from(*normal));
            assert!((vertex.length() - 1.0).abs() < 0.04, "{vertex}");
            assert!(vertex.normalize().dot(normal) > 0.95, "{vertex} {normal}");
        }
        // Each triangle faces where the field grows: out.
        for triangle in surface.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2]
                .map(|corner| glam::Vec3::from(surface.vertices[triangle[corner] as usize]));
            let facing = (b - a).cross(c - a);
            assert!(facing.dot(a + b + c) > 0.0);
        }
        // Every edge is shared by two triangles: no hole.
        let mut edges = std::collections::HashMap::new();
        for triangle in surface.indices.chunks(3) {
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let edge = (triangle[a].min(triangle[b]), triangle[a].max(triangle[b]));
                *edges.entry(edge).or_insert(0) += 1;
            }
        }
        assert!(edges.values().all(|&count| count == 2));
    }

    /// A density's blob, which grows inward, faces out too.
    #[test]
    fn a_density_faces_out() {
        let field = sampled(24, |point| 1.0 / (1.0 + 4.0 * point.length_squared()));
        let surface = isosurface(&field, 0.5).unwrap();
        assert!(!surface.open);
        for (vertex, normal) in surface.vertices.iter().zip(&surface.normals) {
            let (vertex, normal) = (glam::Vec3::from(*vertex), glam::Vec3::from(*normal));
            assert!(vertex.normalize().dot(normal) > 0.9, "{vertex} {normal}");
        }
        for triangle in surface.indices.chunks(3) {
            let [a, b, c] = [0, 1, 2]
                .map(|corner| glam::Vec3::from(surface.vertices[triangle[corner] as usize]));
            assert!((b - a).cross(c - a).dot(a + b + c) > 0.0);
        }
    }

    /// A plane through the box leaves it open, a level the field never
    /// reaches leaves nothing, and a field of the wrong size is refused.
    #[test]
    fn a_cut_surface_is_open_and_a_missing_one_is_an_error() {
        let field = sampled(16, |point| point.y);
        let surface = isosurface(&field, 0.25).unwrap();
        assert!(surface.open);
        assert!(
            surface
                .vertices
                .iter()
                .all(|vertex| (vertex[1] - 0.25).abs() < 1e-4)
        );
        // Most of the walls lie below 0.25: it faces down, all of it.
        assert!(surface.normals.iter().all(|normal| normal[1] < -0.99));
        assert_eq!(
            isosurface(&field, 5.0),
            Err(IsosurfaceError::NoSurface(5.0))
        );
        assert_eq!(
            isosurface(&field, f32::NAN),
            Err(IsosurfaceError::InvalidLevel)
        );
        let short = SampledField {
            values: vec![0.0; 7],
            size: [2, 2, 2],
            ..field
        };
        assert_eq!(isosurface(&short, 0.0), Err(IsosurfaceError::InvalidField));
    }
}
