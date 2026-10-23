//! Displacement fields applied to a drawable and everything in it
//! (`DrawableHandle::deform`, `Anim::deform`).
//!
//! A field is sampled on a grid fixed by the drawable's first deformation,
//! over its world box padded by a quarter of its longest side. Each change
//! adds terms to the drawable's [`gaanim_animation::Deformation`] under one
//! gate, a parameter that goes from 0 to 1 at the change (at once, or as the
//! animation `Anim::deform` returns): the difference between the fixed field
//! before and after, and the scaled fields that leave or arrive. What a
//! drawable is deformed by after each change is kept in [`DeformState`].

use std::sync::Arc;

use gaanim_animation::{DeformLattice, ScalarSource};
use gaanim_core::ObjectId;
use gaanim_core::glam::DVec3;
use gaanim_math::Bounds3D;

use super::measure::BoundsError;
use super::ops::{Op, SharedCanvasState};
use super::types::Anim;
use super::visualization::Parameter;
use super::{Composition, DrawableHandle};

/// Points a field is sampled at along a deformation grid's longest side
/// when the first deformation does not say.
pub const DEFAULT_DEFORM_SAMPLES: usize = 12;
/// Most points along one side of a deformation grid.
pub const MAX_DEFORM_SAMPLES: usize = 64;

/// The grid a drawable's deformations sample their field on: `counts`
/// points from `min` to `max` along each axis, one where the box is flat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeformGrid {
    pub min: DVec3,
    pub max: DVec3,
    pub counts: [usize; 3],
}

impl DeformGrid {
    /// Its points, `x` fastest, then `y`, then `z`: the order a field's
    /// displacements are given in.
    pub fn points(&self) -> Vec<DVec3> {
        let at = |index: usize, axis: usize| {
            if self.counts[axis] < 2 {
                self.min[axis]
            } else {
                self.min[axis]
                    + (self.max[axis] - self.min[axis]) * index as f64
                        / (self.counts[axis] - 1) as f64
            }
        };
        let mut points = Vec::with_capacity(self.len());
        for k in 0..self.counts[2] {
            for j in 0..self.counts[1] {
                for i in 0..self.counts[0] {
                    points.push(DVec3::new(at(i, 0), at(j, 1), at(k, 2)));
                }
            }
        }
        points
    }

    pub fn len(&self) -> usize {
        self.counts.iter().product()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lattice(&self, values: Arc<[DVec3]>) -> Arc<DeformLattice> {
        Arc::new(
            DeformLattice::new(self.min, self.max, self.counts, values)
                .expect("a grid's lattice has one finite value per point"),
        )
    }
}

/// How many points a deformation grid samples its field at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeformSamples {
    /// This many along the box's longest side, the other sides in
    /// proportion (at least two, one where the box is flat).
    Longest(usize),
    /// This many along x, y and z.
    Counts([usize; 3]),
}

#[derive(Debug, thiserror::Error)]
pub enum DeformError {
    #[error("the drawable cannot be measured to place its deformation grid: {0}")]
    Bounds(#[from] BoundsError),
    #[error("samples must be between 1 and {MAX_DEFORM_SAMPLES} along each side")]
    Samples,
    #[error("the region must be finite, with its first corner below its second")]
    Region,
    #[error(
        "a deformation needs one finite displacement per point of its grid ({expected}), got {got}"
    )]
    Values { expected: usize, got: usize },
    #[error("deform needs a drawable's animation proxy")]
    NoTarget,
}

/// What a drawable is deformed by after its last change: its grid, the
/// fixed field, and the fields scaled by an amount that may change.
#[derive(Debug, Clone)]
pub(crate) struct DeformState {
    grid: DeformGrid,
    fixed: Arc<[DVec3]>,
    scaled: Vec<(Arc<[DVec3]>, ScalarSource)>,
}

/// The grid of `target`: the one its first deformation fixed, or a new one
/// over `region`, or over its world box padded by a quarter of its longest
/// side, which it then keeps.
fn grid_for(
    state: &SharedCanvasState,
    target: ObjectId,
    samples: Option<DeformSamples>,
    region: Option<Bounds3D>,
) -> Result<DeformGrid, DeformError> {
    if let Some(known) = state
        .lock()
        .expect("canvas state poisoned")
        .deforms
        .get(&target)
    {
        return Ok(known.grid);
    }
    let (min, max) = match region {
        Some(region) => {
            if !(region.min.is_finite() && region.max.is_finite())
                || region.min.cmpgt(region.max).any()
            {
                return Err(DeformError::Region);
            }
            (region.min, region.max)
        }
        None => {
            let handle = DrawableHandle::existing(state, target).ok_or(DeformError::NoTarget)?;
            let bounds = handle.bounds()?;
            let pad = (bounds.max - bounds.min).max_element() * 0.25;
            let size = bounds.max - bounds.min;
            // A flat side stays flat: one sample across it.
            let padding = DVec3::new(
                if size.x > 1e-9 { pad } else { 0.0 },
                if size.y > 1e-9 { pad } else { 0.0 },
                if size.z > 1e-9 { pad } else { 0.0 },
            );
            (bounds.min - padding, bounds.max + padding)
        }
    };
    let size = max - min;
    let counts = match samples.unwrap_or(DeformSamples::Longest(DEFAULT_DEFORM_SAMPLES)) {
        DeformSamples::Counts(counts) => {
            if counts
                .iter()
                .any(|count| !(1..=MAX_DEFORM_SAMPLES).contains(count))
            {
                return Err(DeformError::Samples);
            }
            counts
        }
        DeformSamples::Longest(longest) => {
            if !(2..=MAX_DEFORM_SAMPLES).contains(&longest) {
                return Err(DeformError::Samples);
            }
            let side = size.max_element();
            std::array::from_fn(|axis| {
                if size[axis] <= 1e-9 || side <= 0.0 {
                    1
                } else {
                    ((longest - 1) as f64 * size[axis] / side).round().max(1.0) as usize + 1
                }
            })
        }
    };
    let grid = DeformGrid { min, max, counts };
    state.lock().expect("canvas state poisoned").deforms.insert(
        target,
        DeformState {
            grid,
            fixed: vec![DVec3::ZERO; grid.len()].into(),
            scaled: Vec::new(),
        },
    );
    Ok(grid)
}

/// Check `values` against `target`'s grid.
fn field_for(
    state: &SharedCanvasState,
    target: ObjectId,
    values: Option<Vec<DVec3>>,
) -> Result<Option<Arc<[DVec3]>>, DeformError> {
    let expected = grid_for(state, target, None, None)?.len();
    let Some(values) = values else {
        return Ok(None);
    };
    if values.len() != expected || values.iter().any(|value| !value.is_finite()) {
        return Err(DeformError::Values {
            expected,
            got: values.len(),
        });
    }
    Ok(Some(values.into()))
}

/// Change what `target` is deformed by to `field` times `scale` (`None`:
/// nothing), under a new gate parameter that the caller takes from 0 to 1.
fn change(
    state: &SharedCanvasState,
    target: ObjectId,
    field: Option<Arc<[DVec3]>>,
    scale: ScalarSource,
) -> Parameter {
    let gate =
        super::visualization::parameter_in(state, 0.0).expect("a parameter starting at 0 is valid");
    // Read before the state is locked: reading it locks the state too.
    let gate_source = gate.source();
    let mut canvas = state.lock().expect("canvas state poisoned");
    let current = canvas
        .deforms
        .get(&target)
        .cloned()
        .expect("a deformation's grid is fixed before it changes");
    let zero: Arc<[DVec3]> = vec![DVec3::ZERO; current.grid.len()].into();
    let (fixed, scaled) = match (field, scale.constant_value()) {
        (None, _) => (zero, Vec::new()),
        (Some(field), Some(amount)) => (
            field
                .iter()
                .map(|value| *value * amount)
                .collect::<Vec<_>>()
                .into(),
            Vec::new(),
        ),
        (Some(field), None) => (zero, vec![(field, scale)]),
    };
    let mut terms: Vec<(Arc<[DVec3]>, ScalarSource)> = Vec::new();
    let difference: Vec<DVec3> = fixed
        .iter()
        .zip(current.fixed.iter())
        .map(|(after, before)| *after - *before)
        .collect();
    if difference.iter().any(|value| *value != DVec3::ZERO) {
        terms.push((difference.into(), 1.0.into()));
    }
    for (field, amount) in &current.scaled {
        let negated: Arc<[DVec3]> = field.iter().map(|value| -*value).collect::<Vec<_>>().into();
        terms.push((negated, amount.clone()));
    }
    terms.extend(scaled.iter().cloned());
    for (values, amount) in terms {
        canvas.active_mut().ops.push(Op::Deform {
            target,
            lattice: current.grid.lattice(values),
            gate: gate_source.clone(),
            scale: amount,
        });
    }
    canvas.deforms.insert(
        target,
        DeformState {
            grid: current.grid,
            fixed,
            scaled,
        },
    );
    gate
}

impl DrawableHandle {
    /// The grid this drawable's deformations sample their field on: fixed
    /// by its first deformation, over `region` or over its world box padded
    /// by a quarter of its longest side, with `samples` points; later calls
    /// return it whatever they ask.
    pub fn deform_grid(
        &self,
        samples: Option<DeformSamples>,
        region: Option<Bounds3D>,
    ) -> Result<DeformGrid, DeformError> {
        grid_for(&self.state, self.id, samples, region)
    }

    /// Deform this drawable and everything in it from the cursor on by
    /// `values` (one displacement per point of [`Self::deform_grid`])
    /// times `scale`, or by nothing with `None`. A `scale` that changes
    /// moves the shape with it.
    pub fn deform(
        self,
        values: Option<Vec<DVec3>>,
        scale: ScalarSource,
    ) -> Result<Self, DeformError> {
        let field = field_for(&self.state, self.id, values)?;
        let gate = change(&self.state, self.id, field, scale);
        let scene = self.scene().ok_or(DeformError::NoTarget)?;
        let at_once = gate.animate().set(1.0).duration(0.0);
        scene
            .lock()
            .expect("scene poisoned")
            .launch_composition_configured(Composition::leaf(at_once), None, None)
            .expect("a scene parameter's animations belong to this scene");
        Ok(self)
    }
}

impl Anim {
    /// The grid the target's deformations sample their field on; see
    /// [`DrawableHandle::deform_grid`].
    pub fn deform_grid(
        &self,
        samples: Option<DeformSamples>,
        region: Option<Bounds3D>,
    ) -> Result<DeformGrid, DeformError> {
        let state = self.owner.as_ref().ok_or(DeformError::NoTarget)?;
        grid_for(state, self.inner.target, samples, region)
    }

    /// Move the target's shape from what it is deformed by now to `values`
    /// (one displacement per point of [`Self::deform_grid`]), or back to
    /// its own shape with `None`: an animation of the change's gate, with
    /// this animation's default duration and easing to chain on.
    pub fn deform(self, values: Option<Vec<DVec3>>) -> Result<Self, DeformError> {
        let state = self.owner.clone().ok_or(DeformError::NoTarget)?;
        let target = self.inner.target;
        let field = field_for(&state, target, values)?;
        Ok(change(&state, target, field, 1.0.into()).animate().set(1.0))
    }
}
