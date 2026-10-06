//! A pivot moved after an object has been shown.

use bevy::prelude::{Entity, World};
use gaanim_core::glam::DVec3;
use gaanim_math::SpatialTransform;

/// Lens that moves a transform's anchor and translation together, from a
/// pose to the same pose about another pivot. It runs as a cut, so playback
/// and seeks only see either end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PivotLens {
    /// Translation and anchor before the change.
    pub from: (DVec3, DVec3),
    /// Translation and anchor after it.
    pub to: (DVec3, DVec3),
}

impl PivotLens {
    pub fn between(from: &SpatialTransform, to: &SpatialTransform) -> Self {
        Self {
            from: (from.translation, from.anchor),
            to: (to.translation, to.anchor),
        }
    }

    /// Translation and anchor `t` of the way through.
    pub fn at(&self, t: f64) -> (DVec3, DVec3) {
        if t >= 1.0 {
            return self.to;
        }
        (
            self.from.0.lerp(self.to.0, t),
            self.from.1.lerp(self.to.1, t),
        )
    }
}

impl gaanim_animation::AnimatableLens for PivotLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let (translation, anchor) = self.at(t);
        if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity)
            && (transform.translation, transform.anchor) != (translation, anchor)
        {
            transform.translation = translation;
            transform.anchor = anchor;
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(*self)
    }

    fn type_name(&self) -> &'static str {
        "Pivot"
    }

    fn history_free(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_animation::AnimatableLens;

    #[test]
    fn the_cut_writes_the_new_pivot_and_its_translation() {
        let from = SpatialTransform::new_2d(2.0, 2.0).scale_uniform(2.0);
        let to = from.about_pivot(DVec3::new(0.0, 2.0, 0.0));
        let mut world = World::new();
        let entity = world.spawn(from).id();
        PivotLens::between(&from, &to).interpolate(&mut world, entity, 1.0);
        assert_eq!(*world.get::<SpatialTransform>(entity).unwrap(), to);
    }
}
