//! Non-destructive path modifiers on a drawable: zig zag, round corners,
//! pucker & bloat, twist, wiggle and offset (TR-04). Each modifier keeps
//! its numbers in hidden `Parameter`s, so they animate like any other.

use gaanim_animation::path_modifiers::ModifierKind;
use gaanim_core::ObjectId;
pub use gaanim_objects::offset::OffsetJoin;

use super::DrawableHandle;
use super::ops::Op;
use super::types::Anim;
use super::visualization::{Parameter, parameter_in};

/// Most ridges per segment of a zig zag.
pub const MAX_RIDGES: u32 = 256;
/// Most wiggle points per segment.
pub const MAX_DETAIL: u32 = 256;
/// Most copies of an offset.
pub const MAX_OFFSET_COPIES: u32 = 64;

/// A modifier added to a drawable's path. Its numbers animate through
/// [`PathModifierHandle::animate`].
#[derive(Debug, Clone)]
pub struct PathModifierHandle {
    kind: ModifierKind,
    params: Vec<(&'static str, Parameter)>,
}

impl PathModifierHandle {
    pub fn kind(&self) -> &ModifierKind {
        &self.kind
    }

    /// Names of the numbers this modifier animates, such as `size`.
    pub fn names(&self) -> Vec<&'static str> {
        self.params.iter().map(|(name, _)| *name).collect()
    }

    fn parameter(&self, name: &str) -> Result<&Parameter, String> {
        self.params
            .iter()
            .find_map(|(known, parameter)| (*known == name).then_some(parameter))
            .ok_or_else(|| {
                format!(
                    "this modifier has no '{name}'; it animates {}",
                    self.names().join(", ")
                )
            })
    }

    /// An animation of the number `name` to `value`.
    pub fn animate(&self, name: &str, value: f64) -> Result<Anim, String> {
        check(&self.kind, name, value)?;
        Ok(self.parameter(name)?.animate().set(value))
    }

    /// Sets the number `name` to `value` from the cursor on, as a cut.
    pub fn set(&self, name: &str, value: f64) -> Result<(), String> {
        check(&self.kind, name, value)?;
        self.parameter(name)?
            .set(value)
            .map_err(|error| error.to_string())
    }
}

/// Whether `value` is a valid `name` for `kind`.
fn check(kind: &ModifierKind, name: &str, value: f64) -> Result<(), String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    match (kind, name) {
        (ModifierKind::RoundCorners, "radius") if value < 0.0 => Err("radius must be >= 0".into()),
        (ModifierKind::PuckerBloat, "amount") if !(-1.0..=1.0).contains(&value) => {
            Err("amount must be within [-1, 1]".into())
        }
        (ModifierKind::Wiggle { .. }, "size") if value < 0.0 => Err("size must be >= 0".into()),
        (ModifierKind::Wiggle { .. }, "frequency") if value < 0.0 => {
            Err("frequency must be >= 0".into())
        }
        _ => Ok(()),
    }
}

impl DrawableHandle {
    /// Adds `kind` to this drawable's modifier stack with the numbers
    /// `params`, applied from the cursor on. Each member of a group or text
    /// is modified on its own.
    pub fn path_modifier(
        &self,
        kind: ModifierKind,
        params: Vec<(&'static str, f64)>,
    ) -> Result<PathModifierHandle, String> {
        for (name, value) in &params {
            check(&kind, name, *value)?;
        }
        let params = params
            .into_iter()
            .map(|(name, value)| {
                parameter_in(&self.state, value)
                    .map(|parameter| (name, parameter))
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<ObjectId> = params
            .iter()
            .map(|(_, parameter)| parameter.drawable().id)
            .collect();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::PathModifier {
                target: self.id,
                kind: kind.clone(),
                params: ids,
            });
        Ok(PathModifierHandle { kind, params })
    }

    /// Zig zag: `ridges` peaks per segment, `size` to each side.
    pub fn zigzag(
        &self,
        size: f64,
        ridges: u32,
        smooth: bool,
    ) -> Result<PathModifierHandle, String> {
        if !(1..=MAX_RIDGES).contains(&ridges) {
            return Err(format!("ridges must be within 1..={MAX_RIDGES}"));
        }
        self.path_modifier(
            ModifierKind::ZigZag { ridges, smooth },
            vec![("size", size)],
        )
    }

    /// Rounds the corners between straight segments with `radius`.
    pub fn round_corners(&self, radius: f64) -> Result<PathModifierHandle, String> {
        self.path_modifier(ModifierKind::RoundCorners, vec![("radius", radius)])
    }

    /// Pucker (negative) or bloat (positive), `amount` within [-1, 1].
    pub fn pucker_bloat(&self, amount: f64) -> Result<PathModifierHandle, String> {
        self.path_modifier(ModifierKind::PuckerBloat, vec![("amount", amount)])
    }

    /// Twists the path by `angle` radians at its center.
    pub fn twist(&self, angle: f64) -> Result<PathModifierHandle, String> {
        self.path_modifier(ModifierKind::Twist, vec![("angle", angle)])
    }

    /// Wiggle: `detail` points per segment moved by seeded noise of up to
    /// `size`, changing `frequency` times a second.
    pub fn wiggle_path(
        &self,
        size: f64,
        detail: u32,
        frequency: f64,
        seed: u64,
    ) -> Result<PathModifierHandle, String> {
        if !(1..=MAX_DETAIL).contains(&detail) {
            return Err(format!("detail must be within 1..={MAX_DETAIL}"));
        }
        self.path_modifier(
            ModifierKind::Wiggle { detail, seed },
            vec![("size", size), ("frequency", frequency)],
        )
    }

    /// Offsets the path by `amount` (inward when negative), `copies` times.
    pub fn offset_path(
        &self,
        amount: f64,
        join: OffsetJoin,
        copies: u32,
    ) -> Result<PathModifierHandle, String> {
        if !(1..=MAX_OFFSET_COPIES).contains(&copies) {
            return Err(format!("copies must be within 1..={MAX_OFFSET_COPIES}"));
        }
        self.path_modifier(
            ModifierKind::Offset { join, copies },
            vec![("amount", amount)],
        )
    }
}
