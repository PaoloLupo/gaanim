//! Copies of a mesh drawn at many points, on the timeline: their reveal,
//! which grows them in one after another, and a morph from one set of
//! copies to another as many.
//!
//! Both lenses write the drawable's [`MeshInstances`] as a function of time
//! alone, so seeks land exactly, and write nothing for an instant the
//! drawable already shows: a cloud that holds still uploads nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::prelude::{Component, Entity, World};
use gaanim_core::glam::{Quat, Vec3};
use gaanim_math::Bounds3D;
use gaanim_scene::{LocalBounds, MeshCopies, MeshInstances, TriangleMeshData};

/// `color` in linear light, as the renderer shades the colors of copies.
pub fn linear(color: gaanim_core::peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    let channel = |value: f32| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    [channel(r), channel(g), channel(b), a]
}

/// Why copies of a mesh could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum InstancesError {
    #[error("the mesh to copy must belong to this scene")]
    ForeignPrototype,
    #[error(
        "only the mesh of a 3D primitive (not an extrusion, a 3D line or copies) can be copied"
    )]
    NotAPrimitive,
    #[error("copies need at least one position")]
    NoPositions,
    #[error("every list of copies needs one entry per position")]
    LengthMismatch,
    #[error("positions, scales, turns and colors must be finite, and turns not zero")]
    NotFinite,
    #[error(transparent)]
    Mesh(#[from] gaanim_objects::primitives3d::Primitive3DError),
}

/// The box holding `mesh` at every one of `copies`, in the drawable's
/// coordinates.
pub(crate) fn copies_bounds(mesh: &TriangleMeshData, copies: &MeshCopies) -> Bounds3D {
    let (min, max) = mesh.vertices.iter().fold(
        (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
        |(min, max), vertex| {
            let point = Vec3::from_array(*vertex);
            (min.min(point), max.max(point))
        },
    );
    let mesh = if min.x.is_finite() {
        (min, max)
    } else {
        (Vec3::ZERO, Vec3::ZERO)
    };
    match copies.bounds(mesh) {
        Some((min, max)) => Bounds3D::new(min.as_dvec3(), max.as_dvec3()),
        None => Bounds3D::default(),
    }
}

/// Grows a drawable's copies in, one after another, from `from` to `to`
/// of the way (see [`MeshInstances::shown`]): 0 to 1 for `create()`, 1 to 0
/// for `uncreate()`.
#[derive(Debug, Clone)]
pub(crate) struct InstanceRevealLens {
    pub from: f32,
    pub to: f32,
}

impl gaanim_animation::AnimatableLens for InstanceRevealLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let reveal = (self.from + (self.to - self.from) * t as f32).clamp(0.0, 1.0);
        if let Some(mut instances) = world.get_mut::<MeshInstances>(entity)
            && instances.reveal != reveal
        {
            instances.reveal = reveal;
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "InstanceReveal"
    }

    /// Before `create()` the copies are hidden; before `uncreate()`, shown.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("instance_reveal")
    }

    fn history_free(&self) -> bool {
        true
    }
}

/// Tells copy morphs apart, as long as the process runs.
static NEXT_MORPH: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct CopiesMorph {
    id: u64,
    from: Arc<MeshCopies>,
    to: Arc<MeshCopies>,
    /// The drawable's mesh, which bounds every copy.
    mesh: Arc<TriangleMeshData>,
}

/// Moves each of a drawable's copies to the copy of the same place in
/// another set: positions, scales and colors along the line between
/// theirs, turns along the shorter arc.
#[derive(Debug, Clone)]
pub(crate) struct CopiesMorphLens(Arc<CopiesMorph>);

/// The copy morph and instant whose copies a drawable shows.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) struct ShownCopies {
    morph: u64,
    t: u64,
    bounds: Bounds3D,
}

impl CopiesMorphLens {
    /// `None` unless both sets hold as many copies.
    pub(crate) fn new(
        from: Arc<MeshCopies>,
        to: Arc<MeshCopies>,
        mesh: Arc<TriangleMeshData>,
    ) -> Option<Self> {
        (from.positions.len() == to.positions.len()).then(|| {
            Self(Arc::new(CopiesMorph {
                id: NEXT_MORPH.fetch_add(1, Ordering::Relaxed),
                from,
                to,
                mesh,
            }))
        })
    }

    /// The copies `t` of the way: each set exactly at its end.
    fn copies_at(&self, t: f32) -> Arc<MeshCopies> {
        let morph = &*self.0;
        if t == 0.0 {
            return Arc::clone(&morph.from);
        }
        if t == 1.0 {
            return Arc::clone(&morph.to);
        }
        let (from, to) = (&*morph.from, &*morph.to);
        let lerp = |a: &[[f32; 3]], b: &[[f32; 3]]| -> Vec<[f32; 3]> {
            a.iter()
                .zip(b)
                .map(|(a, b)| {
                    Vec3::from_array(*a)
                        .lerp(Vec3::from_array(*b), t)
                        .to_array()
                })
                .collect()
        };
        // A list one set lacks counts as its neutral entries there.
        let either = |a: &Option<Vec<[f32; 3]>>, b: &Option<Vec<[f32; 3]>>, neutral: [f32; 3]| {
            match (a, b) {
                (None, None) => None,
                (a, b) => {
                    let filled = |list: &Option<Vec<[f32; 3]>>| {
                        list.clone()
                            .unwrap_or_else(|| vec![neutral; from.positions.len()])
                    };
                    Some(lerp(&filled(a), &filled(b)))
                }
            }
        };
        let rotations = match (&from.rotations, &to.rotations) {
            (None, None) => None,
            (a, b) => {
                let quaternion = |list: &Option<Vec<[f32; 4]>>, index: usize| {
                    list.as_ref().map_or(Quat::IDENTITY, |list| {
                        Quat::from_array(list[index]).normalize()
                    })
                };
                Some(
                    (0..from.positions.len())
                        .map(|index| {
                            quaternion(a, index)
                                .slerp(quaternion(b, index), t)
                                .to_array()
                        })
                        .collect(),
                )
            }
        };
        let colors = match (&from.colors, &to.colors) {
            (None, None) => None,
            (a, b) => {
                let white = [1.0; 4];
                let color = |list: &Option<Vec<[f32; 4]>>, index: usize| {
                    list.as_ref().map_or(white, |list| list[index])
                };
                Some(
                    (0..from.positions.len())
                        .map(|index| {
                            let (a, b) = (color(a, index), color(b, index));
                            std::array::from_fn(|k| a[k] + (b[k] - a[k]) * t)
                        })
                        .collect(),
                )
            }
        };
        Arc::new(MeshCopies {
            positions: lerp(&from.positions, &to.positions),
            scales: either(&from.scales, &to.scales, [1.0; 3]),
            rotations,
            colors,
        })
    }

    fn write(&self, world: &mut World, entity: Entity, t: f64) {
        let (morph, instant) = (self.0.id, t.to_bits());
        if let Some(shown) = world.get::<ShownCopies>(entity).copied()
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
        let copies = self.copies_at(t as f32);
        let bounds = copies_bounds(&self.0.mesh, &copies);
        match drawable.get_mut::<MeshInstances>() {
            Some(mut instances) => instances.copies = copies,
            None => return,
        }
        if let Some(mut local) = drawable.get_mut::<LocalBounds>()
            && local.0 != bounds
        {
            local.0 = bounds;
        }
        drawable.insert(ShownCopies {
            morph,
            t: instant,
            bounds,
        });
    }
}

impl gaanim_animation::AnimatableLens for CopiesMorphLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        self.write(world, entity, t);
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(self.clone())
    }

    fn type_name(&self) -> &'static str {
        "CopiesMorph"
    }

    /// Before its first morph, a drawable shows its own copies.
    fn hold_channel(&self) -> Option<&'static str> {
        Some("copies_morph")
    }

    fn hold(&self, world: &mut World, entity: Entity, _initial: f64) {
        self.write(world, entity, 0.0);
    }

    fn history_free(&self) -> bool {
        true
    }
}
