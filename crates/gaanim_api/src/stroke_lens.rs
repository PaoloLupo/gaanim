//! Interpolation of stroke geometry that has no dedicated timeline lens.

use bevy::prelude::{Entity, World};
use gaanim_scene::StrokeBrush;

/// Lens between two dash offsets of a stroke, in scene units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashOffsetLens {
    pub from: f64,
    pub to: f64,
}

impl DashOffsetLens {
    pub fn at(&self, t: f64) -> f64 {
        self.from + (self.to - self.from) * t
    }
}

impl gaanim_animation::AnimatableLens for DashOffsetLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        if let Some(mut stroke) = world.get_mut::<StrokeBrush>(entity) {
            stroke.style.dash_offset = self.at(t);
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(*self)
    }

    fn type_name(&self) -> &'static str {
        "DashOffset"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_animation::AnimatableLens;

    #[test]
    fn dash_offset_lens_writes_the_stroke() {
        let mut world = World::new();
        let entity = world
            .spawn(StrokeBrush::new(gaanim_core::peniko::Color::WHITE, 0.1))
            .id();
        let lens = DashOffsetLens { from: 0.5, to: 2.5 };
        lens.interpolate(&mut world, entity, 0.25);
        assert_eq!(
            world.get::<StrokeBrush>(entity).unwrap().style.dash_offset,
            1.0
        );
        assert_eq!(lens.at(1.0), 2.5);
    }
}
