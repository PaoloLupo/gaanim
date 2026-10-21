//! A 3D mesh morphing into another, as a timeline clip.
//!
//! `transform_to` between two 3D meshes plays a [`MeshMorphLens`], which
//! writes the source drawable's mesh as a function of time alone: its own
//! mesh before the clip, the target's after it, and between them both
//! meshes sampled on one grid ([`MeshMorph`]) and blended. A frame that
//! shows the instant the drawable already shows writes nothing, so the
//! renderer uploads, and a recording stores, the mesh only while it moves.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::{Component, Entity, World};
use gaanim_core::glam::DVec3;
use gaanim_math::Bounds3D;
use gaanim_objects::morph3d::MeshMorph;
use gaanim_scene::{LocalBounds, TriangleMeshData};

/// Tells morphs apart, as long as the process runs: what a drawable shows
/// is known by its morph and instant.
static NEXT_MORPH: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct Morph {
    id: u64,
    from: Arc<TriangleMeshData>,
    to: Arc<TriangleMeshData>,
    bounds: (Bounds3D, Bounds3D),
    /// `None` when either mesh has no triangle to sample: the mesh then
    /// changes at the end.
    grid: Option<MeshMorph>,
}

/// Morphs a drawable's mesh from one mesh into another.
#[derive(Debug, Clone)]
pub(crate) struct MeshMorphLens(Arc<Morph>);

/// The morph and instant whose mesh a drawable shows, and the bounds of
/// that mesh, which restoring a snapshot may have put back.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShownMorph {
    morph: u64,
    t: u64,
    bounds: Bounds3D,
}

fn mesh_bounds(vertices: &[[f32; 3]]) -> Bounds3D {
    let (min, max) = vertices.iter().fold(
        (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY)),
        |(min, max), vertex| {
            let point = DVec3::new(vertex[0].into(), vertex[1].into(), vertex[2].into());
            (min.min(point), max.max(point))
        },
    );
    if min.x.is_finite() {
        Bounds3D::new(min, max)
    } else {
        Bounds3D::default()
    }
}

/// The color a mesh shows where it has no vertex colors: its material's,
/// or its own.
fn base_color(mesh: &TriangleMeshData) -> [f32; 4] {
    let color = mesh
        .material
        .map(|material| material.color)
        .or(mesh.color)
        .unwrap_or(gaanim_core::peniko::Color::WHITE)
        .to_rgba8();
    [color.r, color.g, color.b, color.a].map(|channel| f32::from(channel) / 255.0)
}

impl MeshMorphLens {
    pub(crate) fn new(from: Arc<TriangleMeshData>, to: Arc<TriangleMeshData>) -> Self {
        let grid = MeshMorph::new(&from, &to, (base_color(&from), base_color(&to)));
        Self(Arc::new(Morph {
            id: NEXT_MORPH.fetch_add(1, Ordering::Relaxed),
            bounds: (mesh_bounds(&from.vertices), mesh_bounds(&to.vertices)),
            from,
            to,
            grid,
        }))
    }

    /// The mesh `t` of the way through the morph and its bounds: each mesh
    /// exactly at its end, the grid between them.
    fn mesh_at(&self, t: f64) -> (TriangleMeshData, Bounds3D) {
        let morph = &*self.0;
        let from = || ((*morph.from).clone(), morph.bounds.0);
        let to = || ((*morph.to).clone(), morph.bounds.1);
        let grid = match &morph.grid {
            _ if t == 0.0 => return from(),
            _ if t == 1.0 => return to(),
            None if t < 1.0 => return from(),
            None => return to(),
            Some(grid) => grid,
        };
        let frame = grid.at(t as f32);
        let mesh = TriangleMeshData {
            vertices: frame.vertices,
            indices: frame.indices,
            normals: Some(frame.normals),
            uvs: None,
            color: if t < 0.5 {
                morph.from.color
            } else {
                morph.to.color
            },
            colors: frame.colors,
            material: morph.from.material,
            surface: None,
        };
        let bounds = Bounds3D::new(frame.min.as_dvec3(), frame.max.as_dvec3());
        (mesh, bounds)
    }

    fn write(&self, world: &mut World, entity: Entity, t: f64) {
        let (morph, instant) = (self.0.id, t.to_bits());
        if let Some(shown) = world.get::<ShownMorph>(entity).copied()
            && (shown.morph, shown.t) == (morph, instant)
        {
            if let Some(mut bounds) = world.get_mut::<LocalBounds>(entity)
                && bounds.0 != shown.bounds
            {
                bounds.0 = shown.bounds;
            }
            return;
        }
        let Ok(mut drawable) = world.get_entity_mut(entity) else {
            return;
        };
        let (mesh, bounds) = self.mesh_at(t);
        match drawable.get_mut::<TriangleMeshData>() {
            Some(mut shown) => *shown = mesh,
            None => return,
        }
        if let Some(mut local) = drawable.get_mut::<LocalBounds>()
            && local.0 != bounds
        {
            local.0 = bounds;
        }
        drawable.insert(ShownMorph {
            morph,
            t: instant,
            bounds,
        });
    }
}

impl gaanim_animation::AnimatableLens for MeshMorphLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        self.write(world, entity, t);
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "MeshMorph"
    }

    /// Before its first morph, a drawable shows its own mesh.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("mesh_morph")
    }

    fn hold(&self, world: &mut World, entity: Entity, _initial: f64) {
        self.write(world, entity, 0.0);
    }

    fn history_free(&self) -> bool {
        true
    }
}
