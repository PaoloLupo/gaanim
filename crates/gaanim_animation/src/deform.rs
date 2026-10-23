//! Displacement fields that deform a drawable and everything in it
//! (`Drawable.deform`).
//!
//! A field is sampled once, on a grid over a box of the world, and read
//! between its points trilinearly. A [`Deformation`] adds up terms, each a
//! grid of displacements weighed by a gate (a parameter the timeline moves
//! from 0 to 1, at once or over an animation) and a scale (an amount, which
//! may be a parameter too), so a seek finds the same shape as playback.
//!
//! Each frame the displacement is applied to the drawable's members the way
//! each is drawn: the vertices of meshes and line lists move one by one, a
//! 3D line that follows other objects follows them, and any other member (a
//! point, a text, a billboard, a 2D drawable in the 3D world) moves as a
//! whole by the displacement at its origin. Members move before 3D lines
//! read where their endpoints are and are put back after propagation;
//! meshes and lines are deformed after propagation and put back at the
//! start of the next frame, so authored values never change.

use std::collections::HashSet;
use std::sync::Arc;

use bevy::prelude::*;
use gaanim_core::glam::{DMat3, DVec3};
use gaanim_math::{GlobalSpatialTransform, SpatialTransform};
use gaanim_scene::{Billboard, GroupMarker, InWorld3D, LineListData, TriangleMeshData};

use crate::updaters::{TrackingEndpoint, TrackingLine3D, TrackingScalar, entity_world_matrix};

/// A displacement field over a box of the world: one vector per point of a
/// grid of `counts` points, `x` fastest, read between them trilinearly and
/// held at the grid's faces beyond it.
#[derive(Debug, Clone, PartialEq)]
pub struct DeformLattice {
    pub min: DVec3,
    pub max: DVec3,
    pub counts: [usize; 3],
    pub values: Arc<[DVec3]>,
}

impl DeformLattice {
    /// A lattice of `values` over `min..max`; `None` when the counts do not
    /// match the values, a count is zero or a value or corner is not finite.
    pub fn new(min: DVec3, max: DVec3, counts: [usize; 3], values: Arc<[DVec3]>) -> Option<Self> {
        let finite = min.is_finite() && max.is_finite() && values.iter().all(|v| v.is_finite());
        let size = counts.iter().product::<usize>();
        (finite && size > 0 && size == values.len() && min.cmple(max).all()).then_some(Self {
            min,
            max,
            counts,
            values,
        })
    }

    fn value(&self, i: usize, j: usize, k: usize) -> DVec3 {
        self.values[i + self.counts[0] * (j + self.counts[1] * k)]
    }

    /// Along `axis`: the lower grid index around `p`, the fraction of the
    /// way to the next one, and the spacing (zero on a single point).
    fn cell(&self, p: DVec3, axis: usize) -> (usize, f64, f64) {
        let count = self.counts[axis];
        let span = self.max[axis] - self.min[axis];
        if count < 2 || span <= 0.0 {
            return (0, 0.0, 0.0);
        }
        let step = span / (count - 1) as f64;
        let position = ((p[axis] - self.min[axis]) / step).clamp(0.0, (count - 1) as f64);
        let index = (position.floor() as usize).min(count - 2);
        (index, position - index as f64, step)
    }

    /// The displacement at `p`.
    pub fn sample(&self, p: DVec3) -> DVec3 {
        self.sample_with_gradient(p).0
    }

    /// The displacement at `p` and its derivative: column `a` is how it
    /// changes along axis `a`, zero beyond the grid and across a single
    /// point.
    pub fn sample_with_gradient(&self, p: DVec3) -> (DVec3, DMat3) {
        let cells = [self.cell(p, 0), self.cell(p, 1), self.cell(p, 2)];
        let next = |axis: usize, index: usize| (index + 1).min(self.counts[axis] - 1);
        let mut value = DVec3::ZERO;
        let mut derivative = [DVec3::ZERO; 3];
        for corner in 0..8 {
            let pick = [corner & 1, (corner >> 1) & 1, (corner >> 2) & 1];
            let index: [usize; 3] = std::array::from_fn(|axis| {
                let (low, _, _) = cells[axis];
                if pick[axis] == 1 {
                    next(axis, low)
                } else {
                    low
                }
            });
            let weight = |axis: usize| {
                let (_, t, _) = cells[axis];
                if pick[axis] == 1 { t } else { 1.0 - t }
            };
            let corner_value = self.value(index[0], index[1], index[2]);
            value += corner_value * weight(0) * weight(1) * weight(2);
            for axis in 0..3 {
                let (_, t, step) = cells[axis];
                // Inside the grid the weight changes by ±1 per step; at a
                // face it is held, and so is the field.
                let inside = step > 0.0
                    && p[axis] > self.min[axis]
                    && p[axis] < self.max[axis]
                    && t.is_finite();
                if !inside {
                    continue;
                }
                let slope = if pick[axis] == 1 { 1.0 } else { -1.0 } / step;
                let others: f64 = (0..3).filter(|other| *other != axis).map(weight).product();
                derivative[axis] += corner_value * slope * others;
            }
        }
        (
            value,
            DMat3::from_cols(derivative[0], derivative[1], derivative[2]),
        )
    }
}

/// One term of a [`Deformation`]: `lattice` weighed by `gate` and `scale`.
#[derive(Debug, Clone)]
pub struct DeformTerm {
    pub lattice: Arc<DeformLattice>,
    /// 0 before the term applies and 1 once it does, possibly animated.
    pub gate: TrackingScalar,
    /// How much of the lattice applies: 1, or an amount that may change.
    pub scale: TrackingScalar,
}

/// The displacement field a drawable and everything in it are deformed by.
#[derive(Component, Debug, Clone, Default)]
pub struct Deformation {
    pub terms: Vec<DeformTerm>,
}

impl Deformation {
    /// The weight of each term now; a term that cannot be read weighs 0.
    fn weights(&self, world: &World) -> Vec<f64> {
        self.terms
            .iter()
            .map(|term| {
                let weight = term.gate.evaluate(world).unwrap_or(0.0)
                    * term.scale.evaluate(world).unwrap_or(0.0);
                if weight.is_finite() { weight } else { 0.0 }
            })
            .collect()
    }

    /// The displacement at `p` under `weights`, and its derivative.
    fn displacement(&self, weights: &[f64], p: DVec3) -> (DVec3, DMat3) {
        let mut value = DVec3::ZERO;
        let mut derivative = DMat3::ZERO;
        for (term, weight) in self.terms.iter().zip(weights) {
            if *weight == 0.0 {
                continue;
            }
            let (term_value, term_derivative) = term.lattice.sample_with_gradient(p);
            value += term_value * *weight;
            derivative += term_derivative * *weight;
        }
        (value, derivative)
    }
}

/// How a member of a deformed drawable takes the displacement.
enum Member {
    /// It moves as a whole, by the displacement at its origin.
    Rigid(Entity),
    /// Its vertices move one by one.
    Vertices(Entity),
}

/// The members of `root` and how each takes its deformation. A group passes
/// it to its members; a 3D line that follows other objects follows them;
/// what such a line follows moves as a whole, as a node of a frame does, so
/// the line stays on it; another mesh or line list takes it per vertex and
/// passes it to its children; anything else moves as a whole, with its
/// children.
fn members(root: Entity, world: &World, followed: &HashSet<Entity>, out: &mut Vec<Member>) {
    let follows = world.get::<TrackingLine3D>(root).is_some_and(|line| {
        [&line.from, &line.to]
            .iter()
            .any(|endpoint| !matches!(endpoint, TrackingEndpoint::Static(_)))
    });
    let children = || world.get::<Children>(root).into_iter().flatten().copied();
    if follows {
        return;
    }
    if followed.contains(&root) {
        out.push(Member::Rigid(root));
    } else if world.get::<TriangleMeshData>(root).is_some()
        || world.get::<LineListData>(root).is_some()
    {
        out.push(Member::Vertices(root));
        for child in children() {
            members(child, world, followed, out);
        }
    } else if world.get::<GroupMarker>(root).is_some()
        && world.get::<InWorld3D>(root).is_none()
        && world.get::<Billboard>(root).is_none()
    {
        for child in children() {
            members(child, world, followed, out);
        }
    } else {
        out.push(Member::Rigid(root));
    }
}

/// The deformations to apply this frame: each deformed drawable with its
/// members and the weights of its terms, leaving out those that weigh
/// nothing.
fn active(world: &mut World) -> Vec<(Deformation, Vec<f64>, Vec<Member>)> {
    let mut query = world.query::<(Entity, &Deformation)>();
    let deformed: Vec<(Entity, Deformation)> = query
        .iter(world)
        .map(|(entity, deformation)| (entity, deformation.clone()))
        .collect();
    if deformed.is_empty() {
        return Vec::new();
    }
    // What 3D lines follow, which moves as a whole so they stay on it.
    let mut lines = world.query::<&TrackingLine3D>();
    let followed: HashSet<Entity> = lines
        .iter(world)
        .flat_map(|line| [&line.from, &line.to])
        .filter_map(|endpoint| match endpoint {
            TrackingEndpoint::Entity(entity) | TrackingEndpoint::EntityAnchor { entity, .. } => {
                Some(*entity)
            }
            _ => None,
        })
        .collect();
    deformed
        .into_iter()
        .filter_map(|(entity, deformation)| {
            let weights = deformation.weights(world);
            if weights.iter().all(|weight| *weight == 0.0) {
                return None;
            }
            let mut found = Vec::new();
            members(entity, world, &followed, &mut found);
            Some((deformation, weights, found))
        })
        .collect()
}

/// Translations of members moved as a whole this frame, as authored, to put
/// back after propagation. Like [`DeformedGeometry`], it exists only once a
/// scene deforms something: a resource is an entity, and one in every scene
/// would shift the others.
#[derive(Resource, Default)]
pub struct DeformedTranslations(Vec<(Entity, DVec3)>);

/// Meshes and line lists deformed this frame, as authored, to put back at
/// the start of the next frame.
#[derive(Resource, Default)]
pub struct DeformedGeometry {
    meshes: Vec<(Entity, TriangleMeshData)>,
    lines: Vec<(Entity, LineListData)>,
}

/// Move the members of deformed drawables that move as a whole, before 3D
/// lines read where their endpoints are.
pub fn apply_deformation_offsets_system(world: &mut World) {
    let mut moved = Vec::new();
    for (deformation, weights, found) in active(world) {
        for member in found {
            let Member::Rigid(entity) = member else {
                continue;
            };
            let Some(origin) = entity_world_matrix(entity, world)
                .map(|matrix| matrix.transform_point3(DVec3::ZERO))
            else {
                continue;
            };
            let (offset, _) = deformation.displacement(&weights, origin);
            // The offset in the parent's space, where the translation lives.
            let parent = world
                .get::<ChildOf>(entity)
                .and_then(|relation| entity_world_matrix(relation.parent(), world));
            let local = match parent {
                Some(matrix) => matrix.inverse().transform_vector3(offset),
                None => offset,
            };
            if !local.is_finite() || local == DVec3::ZERO {
                continue;
            }
            if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                moved.push((entity, transform.translation));
                transform.translation += local;
            }
        }
    }
    if !moved.is_empty() || world.contains_resource::<DeformedTranslations>() {
        world.insert_resource(DeformedTranslations(moved));
    }
}

/// Put back the translations [`apply_deformation_offsets_system`] moved,
/// once propagation has carried them to the world transforms.
pub fn restore_deformation_offsets_system(world: &mut World) {
    let Some(mut moved) = world.get_resource_mut::<DeformedTranslations>() else {
        return;
    };
    let moved = std::mem::take(&mut moved.0);
    for (entity, translation) in moved {
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
            transform.translation = translation;
        }
    }
}

/// Deform the vertices of the meshes and line lists of deformed drawables,
/// in the world, once their world transforms are known.
pub fn apply_deformation_vertices_system(world: &mut World) {
    let mut geometry = DeformedGeometry::default();
    for (deformation, weights, found) in active(world) {
        for member in found {
            let Member::Vertices(entity) = member else {
                continue;
            };
            let Some(global) = world
                .get::<GlobalSpatialTransform>(entity)
                .map(|global| global.mat4)
            else {
                continue;
            };
            let inverse = global.inverse();
            // A local point's place in the world after the displacement,
            // back in local space; normals follow the map's derivative.
            let linear = gaanim_core::glam::DMat3::from_mat4(global);
            let moved = |vertex: [f32; 3]| {
                let point = global.transform_point3(DVec3::from_array(vertex.map(f64::from)));
                let (offset, derivative) = deformation.displacement(&weights, point);
                let local = inverse.transform_point3(point + offset);
                (local, derivative)
            };
            if let Some(mesh) = world.get::<TriangleMeshData>(entity).cloned() {
                let mut deformed = mesh.clone();
                // A morph frame's blend would rebuild the undeformed mesh.
                deformed.blend = None;
                let mut normals = deformed.normals.take();
                for (index, vertex) in deformed.vertices.iter_mut().enumerate() {
                    let (local, derivative) = moved(*vertex);
                    *vertex = local.as_vec3().to_array();
                    if let Some(normal) =
                        normals.as_mut().and_then(|normals| normals.get_mut(index))
                    {
                        // The local map's derivative is A⁻¹ (I + D) A; a
                        // normal takes its inverse transpose.
                        let jacobian = linear.inverse() * (DMat3::IDENTITY + derivative) * linear;
                        let turned = jacobian.inverse().transpose()
                            * DVec3::from_array(normal.map(f64::from));
                        if let Some(unit) = turned.try_normalize() {
                            *normal = unit.as_vec3().to_array();
                        }
                    }
                }
                deformed.normals = normals;
                geometry.meshes.push((entity, mesh));
                world.entity_mut(entity).insert(deformed);
            } else if let Some(lines) = world.get::<LineListData>(entity).cloned() {
                let mut deformed = lines.clone();
                for point in &mut deformed.points {
                    *point = moved(*point).0.as_vec3().to_array();
                }
                geometry.lines.push((entity, lines));
                world.entity_mut(entity).insert(deformed);
            }
        }
    }
    if !(geometry.meshes.is_empty() && geometry.lines.is_empty())
        || world.contains_resource::<DeformedGeometry>()
    {
        world.insert_resource(geometry);
    }
}

/// Put back the meshes and line lists deformed last frame, before seeks,
/// tweens and snapshots read them.
pub fn restore_deformation_vertices_system(world: &mut World) {
    let Some(mut geometry) = world.get_resource_mut::<DeformedGeometry>() else {
        return;
    };
    let geometry = std::mem::take(&mut *geometry);
    for (entity, mesh) in geometry.meshes {
        if let Ok(mut target) = world.get_entity_mut(entity) {
            target.insert(mesh);
        }
    }
    for (entity, lines) in geometry.lines {
        if let Ok(mut target) = world.get_entity_mut(entity) {
            target.insert(lines);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lattice(counts: [usize; 3], value: impl Fn(DVec3) -> DVec3) -> DeformLattice {
        let (min, max) = (DVec3::ZERO, DVec3::new(2.0, 4.0, 1.0));
        let mut values = Vec::new();
        for k in 0..counts[2] {
            for j in 0..counts[1] {
                for i in 0..counts[0] {
                    let at = |index: usize, axis: usize| {
                        if counts[axis] < 2 {
                            min[axis]
                        } else {
                            min[axis]
                                + (max[axis] - min[axis]) * index as f64 / (counts[axis] - 1) as f64
                        }
                    };
                    values.push(value(DVec3::new(at(i, 0), at(j, 1), at(k, 2))));
                }
            }
        }
        DeformLattice::new(min, max, counts, values.into()).unwrap()
    }

    /// A linear field is read exactly between the points, with its slope,
    /// and held at the faces beyond the grid.
    #[test]
    fn a_lattice_reads_a_linear_field_exactly_with_its_slope() {
        let field = |p: DVec3| DVec3::new(0.5 * p.y, 0.0, p.x - 2.0 * p.z);
        let grid = lattice([3, 5, 2], field);
        let inside = DVec3::new(0.7, 2.9, 0.3);
        let (value, derivative) = grid.sample_with_gradient(inside);
        assert!(value.distance(field(inside)) < 1e-12, "{value:?}");
        let expected = DMat3::from_cols(
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(0.5, 0.0, 0.0),
            DVec3::new(0.0, 0.0, -2.0),
        );
        assert!(
            (derivative - expected)
                .abs()
                .to_cols_array()
                .iter()
                .all(|d| *d < 1e-12),
            "{derivative:?}"
        );
        // Beyond the grid the field is held at its face.
        let beyond = grid.sample(DVec3::new(0.7, 9.0, 0.3));
        assert!(beyond.distance(field(DVec3::new(0.7, 4.0, 0.3))) < 1e-12);
        // A single point along an axis holds the field along it.
        let flat = lattice([3, 5, 1], field);
        assert!(
            flat.sample(DVec3::new(1.0, 2.0, 0.6))
                .distance(field(DVec3::new(1.0, 2.0, 0.0)))
                < 1e-12
        );
        assert!(
            DeformLattice::new(
                DVec3::ZERO,
                DVec3::ONE,
                [2, 2, 2],
                vec![DVec3::ZERO; 7].into()
            )
            .is_none()
        );
    }
}
