//! Billboards hidden behind 3D meshes.
//!
//! A billboard (a 3D label: the tick labels of 3D axes, `.billboard()`
//! text) is 2D content drawn over the 3D image, so on its own it shows even
//! where a surface lies in front of it. [`billboard_occlusion_system`] casts
//! rays from the camera to a few points around each billboard's anchor
//! against the visible meshes and stores the share of them that reaches it
//! as an [`Occlusion`] factor, which the opacity cascade multiplies in: a
//! label fades out as it passes behind a surface. It reads only the scene's
//! state, so seeks, exports and recordings agree.
//!
//! Most billboards are drawn by the 3D pass instead ([`InWorld3D`]), hidden
//! pixel by pixel; only those holding what it does not draw, such as an
//! image or a 2D effect, fade here.

use std::sync::Arc;

use bevy::ecs::change_detection::{DetectChanges, Ref};
use bevy::platform::collections::HashMap;
use bevy::prelude::{Commands, Component, Entity, Local, Query, Res, With, Without};
use gaanim_core::glam::DVec3;
use gaanim_math::GlobalSpatialTransform;

use crate::components::{
    Billboard, GlobalOpacity, HudOverlay, InWorld3D, Mesh3DMarker, TriangleMeshData, Visible,
};

/// The share of a billboard that 3D meshes leave visible, between zero and
/// one, which multiplies into its propagated opacity and its descendants'.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub struct Occlusion(pub f32);

/// How far around a billboard's anchor, across the line of sight, its rays
/// aim, in scene units: a label fades over about this distance as it passes
/// behind an edge.
const SPREAD: f64 = 0.08;

/// A bounding volume hierarchy over a mesh's triangles, in its own space.
#[derive(Debug)]
pub struct MeshBvh {
    triangles: Vec<[DVec3; 3]>,
    nodes: Vec<Node>,
}

#[derive(Debug, Clone, Copy)]
struct Node {
    low: DVec3,
    high: DVec3,
    /// A leaf's triangles, `first..first + count`; an inner node's
    /// children, `first` and `second`, with no triangles of its own.
    first: u32,
    second: u32,
    count: u32,
}

/// Triangles in a leaf, at most.
const LEAF: usize = 4;

impl MeshBvh {
    /// The hierarchy of `mesh`'s valid triangles.
    pub fn new(mesh: &TriangleMeshData) -> Self {
        let point = |index: u32| {
            mesh.vertices
                .get(index as usize)
                .map(|v| DVec3::new(v[0].into(), v[1].into(), v[2].into()))
        };
        let mut triangles: Vec<[DVec3; 3]> = mesh
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .filter_map(|&[a, b, c]| Some([point(a)?, point(b)?, point(c)?]))
            .filter(|corners| corners.iter().all(|p| p.is_finite()))
            .collect();
        let mut nodes = Vec::with_capacity(triangles.len() / LEAF * 2 + 1);
        if !triangles.is_empty() {
            let count = triangles.len();
            build(&mut triangles, 0, count, &mut nodes);
        }
        Self { triangles, nodes }
    }

    /// Whether the segment from `start` to `end` crosses a triangle, short
    /// of `end` by `margin` of its length.
    pub fn blocks(&self, start: DVec3, end: DVec3, margin: f64) -> bool {
        let Some(root) = self.nodes.first() else {
            return false;
        };
        let direction = end - start;
        let limit = 1.0 - margin;
        let inverse = DVec3::ONE / direction;
        if !slab(root.low, root.high, start, inverse, limit) {
            return false;
        }
        let mut stack = vec![0usize];
        while let Some(index) = stack.pop() {
            let node = self.nodes[index];
            if !slab(node.low, node.high, start, inverse, limit) {
                continue;
            }
            if node.count > 0 {
                let first = node.first as usize;
                if self.triangles[first..first + node.count as usize]
                    .iter()
                    .any(|triangle| crosses(triangle, start, direction, limit))
                {
                    return true;
                }
            } else {
                stack.push(node.first as usize);
                stack.push(node.second as usize);
            }
        }
        false
    }
}

/// Build the node of `triangles[first..first + count]` and its subtree,
/// returning its index.
fn build(triangles: &mut [[DVec3; 3]], first: usize, count: usize, nodes: &mut Vec<Node>) -> usize {
    let span = &mut triangles[first..first + count];
    let (low, high) = span.iter().flatten().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(low, high), p| (low.min(*p), high.max(*p)),
    );
    let index = nodes.len();
    nodes.push(Node {
        low,
        high,
        first: first as u32,
        second: 0,
        count: count as u32,
    });
    if count <= LEAF {
        return index;
    }
    // Split at the median along the axis the centroids spread most.
    let centroid = |t: &[DVec3; 3]| (t[0] + t[1] + t[2]) / 3.0;
    let (c_low, c_high) = span.iter().map(centroid).fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(low, high), c| (low.min(c), high.max(c)),
    );
    let extent = c_high - c_low;
    let axis = if extent.x >= extent.y && extent.x >= extent.z {
        0
    } else if extent.y >= extent.z {
        1
    } else {
        2
    };
    let half = count / 2;
    span.select_nth_unstable_by(half, |a, b| centroid(a)[axis].total_cmp(&centroid(b)[axis]));
    let left = build(triangles, first, half, nodes);
    let right = build(triangles, first + half, count - half, nodes);
    nodes[index] = Node {
        first: left as u32,
        second: right as u32,
        count: 0,
        ..nodes[index]
    };
    index
}

/// Whether the ray `start + t * direction`, for `t` in `[0, limit]`, meets
/// the box from `low` to `high`; `inverse` is `1 / direction`.
fn slab(low: DVec3, high: DVec3, start: DVec3, inverse: DVec3, limit: f64) -> bool {
    let a = (low - start) * inverse;
    let b = (high - start) * inverse;
    let near = a.min(b).max_element().max(0.0);
    let far = a.max(b).min_element().min(limit);
    near <= far
}

/// Whether the ray `start + t * direction`, for `t` in `(0, limit)`, crosses
/// `triangle` (Möller–Trumbore).
fn crosses(triangle: &[DVec3; 3], start: DVec3, direction: DVec3, limit: f64) -> bool {
    let [a, b, c] = *triangle;
    let (ab, ac) = (b - a, c - a);
    let p = direction.cross(ac);
    let determinant = ab.dot(p);
    if determinant.abs() < 1e-14 {
        return false;
    }
    let inverse = 1.0 / determinant;
    let s = start - a;
    let u = s.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = s.cross(ab);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = ac.dot(q) * inverse;
    t > 1e-9 && t < limit
}

/// The hierarchies of the meshes, with the change tick they were built at.
#[derive(Default)]
pub struct MeshBvhs(HashMap<Entity, (u32, Arc<MeshBvh>)>);

/// System: the [`Occlusion`] of every billboard, from rays cast from the
/// camera to points around its anchor against the visible meshes. An opaque
/// mesh blocks a ray; a translucent one lets through what its opacity
/// leaves. Billboards keep their factor until it changes, so a static
/// scene does not report changed opacities every frame.
#[allow(clippy::type_complexity)]
pub fn billboard_occlusion_system(
    mut commands: Commands,
    camera: Option<Res<gaanim_math::ResolvedCamera>>,
    billboards: Query<
        (Entity, &GlobalSpatialTransform, Option<&Occlusion>),
        (With<Billboard>, Without<InWorld3D>),
    >,
    meshes: Query<
        (
            Entity,
            Ref<TriangleMeshData>,
            &GlobalSpatialTransform,
            &GlobalOpacity,
        ),
        (With<Visible>, With<Mesh3DMarker>, Without<HudOverlay>),
    >,
    mut cache: Local<MeshBvhs>,
) {
    if billboards.is_empty() {
        cache.0.clear();
        return;
    }
    let Some(camera) = camera else {
        return;
    };
    let perspective = matches!(
        camera.projection,
        gaanim_math::Projection::Perspective { .. }
    );
    let forward = camera.rotation * DVec3::NEG_Z;
    let right = camera.rotation * DVec3::X;
    let up = camera.rotation * DVec3::Y;
    // The meshes, in their own space, with how much each lets through.
    let mut seen = Vec::new();
    let occluders: Vec<(Arc<MeshBvh>, gaanim_core::glam::DMat4, f64)> = meshes
        .iter()
        .filter(|(.., opacity)| opacity.0 > 0.0)
        .map(|(entity, mesh, transform, opacity)| {
            seen.push(entity);
            let tick = mesh.last_changed().get();
            let bvh = match cache.0.get(&entity) {
                Some((built, bvh)) if *built == tick => Arc::clone(bvh),
                _ => {
                    let bvh = Arc::new(MeshBvh::new(&mesh));
                    cache.0.insert(entity, (tick, Arc::clone(&bvh)));
                    bvh
                }
            };
            (
                bvh,
                transform.mat4.inverse(),
                f64::from(opacity.0.clamp(0.0, 1.0)),
            )
        })
        .collect();
    cache.0.retain(|entity, _| seen.contains(entity));
    for (entity, transform, current) in &billboards {
        let anchor = transform.mat4.transform_point3(DVec3::ZERO);
        let visible = if occluders.is_empty() || !anchor.is_finite() {
            1.0
        } else {
            let targets = [
                anchor,
                anchor + right * SPREAD,
                anchor - right * SPREAD,
                anchor + up * SPREAD,
                anchor - up * SPREAD,
            ];
            let reach: f64 = targets
                .iter()
                .map(|target| {
                    let start = if perspective {
                        camera.position
                    } else {
                        // Orthographic rays run along the view, from well
                        // in front of everything.
                        *target - forward * 1.0e4
                    };
                    occluders
                        .iter()
                        .map(|(bvh, inverse, opacity)| {
                            let blocked = bvh.blocks(
                                inverse.transform_point3(start),
                                inverse.transform_point3(*target),
                                1e-3,
                            );
                            if blocked { 1.0 - opacity } else { 1.0 }
                        })
                        .product::<f64>()
                })
                .sum::<f64>()
                / targets.len() as f64;
            // Quantized, so that tiny shifts do not report a change.
            (reach * 64.0).round() / 64.0
        } as f32;
        match current {
            Some(current) if current.0 == visible => {}
            Some(_) => {
                commands.entity(entity).insert(Occlusion(visible));
            }
            None if visible < 1.0 => {
                commands.entity(entity).insert(Occlusion(visible));
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f32) -> TriangleMeshData {
        TriangleMeshData {
            vertices: vec![
                [-1.0, -1.0, z],
                [1.0, -1.0, z],
                [1.0, 1.0, z],
                [-1.0, 1.0, z],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            normals: None,
            uvs: None,
            color: None,
            colors: None,
            material: None,
        }
    }

    #[test]
    fn a_billboard_behind_a_mesh_fades_out_with_its_children() {
        use bevy::prelude::{App, ChildOf};
        use gaanim_math::SpatialTransform;

        use crate::components::Opacity;

        let mut app = App::new();
        app.add_plugins(crate::GaanimScenePlugin);
        let mut camera = gaanim_math::Camera::perspective_3d(640, 360, 0.8);
        camera.position = DVec3::new(0.0, 0.0, 5.0);
        camera.rotation = Default::default();
        app.insert_resource(camera);
        let at = |translation: DVec3| SpatialTransform {
            translation,
            ..Default::default()
        };
        app.world_mut().spawn((
            square(0.0),
            at(DVec3::ZERO),
            GlobalSpatialTransform::default(),
            Visible,
            Mesh3DMarker,
            Opacity(1.0),
            GlobalOpacity(1.0),
        ));
        let label = app
            .world_mut()
            .spawn((
                Billboard,
                at(DVec3::new(0.2, 0.1, -1.0)),
                GlobalSpatialTransform::default(),
                Opacity(1.0),
                GlobalOpacity(1.0),
            ))
            .id();
        let glyph = app
            .world_mut()
            .spawn((
                ChildOf(label),
                at(DVec3::ZERO),
                GlobalSpatialTransform::default(),
                Opacity(1.0),
                GlobalOpacity(1.0),
            ))
            .id();
        app.update();
        app.update();
        let opacity = |app: &App, entity| app.world().get::<GlobalOpacity>(entity).unwrap().0;
        assert_eq!(app.world().get::<Occlusion>(label), Some(&Occlusion(0.0)));
        assert_eq!((opacity(&app, label), opacity(&app, glyph)), (0.0, 0.0));
        // Beside the square it shows again.
        app.world_mut()
            .get_mut::<SpatialTransform>(label)
            .unwrap()
            .translation = DVec3::new(3.0, 0.0, -1.0);
        app.update();
        app.update();
        assert_eq!((opacity(&app, label), opacity(&app, glyph)), (1.0, 1.0));
    }

    #[test]
    fn segments_through_a_mesh_are_blocked_and_others_are_not() {
        let bvh = MeshBvh::new(&square(0.0));
        let eye = DVec3::new(0.0, 0.0, 5.0);
        assert!(bvh.blocks(eye, DVec3::new(0.2, 0.3, -1.0), 1e-3));
        // Short of the square, beside it, or ending on it.
        assert!(!bvh.blocks(eye, DVec3::new(0.0, 0.0, 1.0), 1e-3));
        assert!(!bvh.blocks(eye, DVec3::new(3.0, 0.0, -1.0), 1e-3));
        assert!(!bvh.blocks(eye, DVec3::new(0.0, 0.0, 0.0), 1e-3));
    }

    #[test]
    fn large_meshes_build_a_hierarchy_that_finds_the_same_hits() {
        // A 40 x 40 grid of squares at z = 0, two triangles each.
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for row in 0..=40 {
            for column in 0..=40 {
                vertices.push([column as f32 * 0.1 - 2.0, row as f32 * 0.1 - 2.0, 0.0]);
            }
        }
        for row in 0..40u32 {
            for column in 0..40u32 {
                let a = row * 41 + column;
                indices.extend_from_slice(&[a, a + 1, a + 42, a, a + 42, a + 41]);
            }
        }
        let grid = TriangleMeshData {
            vertices,
            indices,
            ..square(0.0)
        };
        let bvh = MeshBvh::new(&grid);
        assert!(bvh.nodes.len() > 1);
        let eye = DVec3::new(0.3, -0.2, 4.0);
        for target in [
            DVec3::new(1.95, 1.95, -0.5),
            DVec3::new(-1.9, 0.4, -2.0),
            DVec3::new(0.05, -1.99, -0.1),
        ] {
            assert!(bvh.blocks(eye, target, 1e-3), "{target}");
        }
        assert!(!bvh.blocks(eye, DVec3::new(2.5, 0.0, -1.0), 1e-3));
    }
}
