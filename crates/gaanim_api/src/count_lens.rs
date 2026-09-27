//! Visibility of the copies of a repeater while its `count` animates.

use bevy::prelude::{Entity, World};
use gaanim_scene::Presence;

/// Presence of copy `index` when `count` copies are shown: whole copies
/// below `count` are fully visible and the fractional part fades the next.
pub fn presence(count: f64, index: usize) -> f32 {
    (count - index as f64).clamp(0.0, 1.0) as f32
}

/// Lens of one repeater copy between two counts of its group.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CountLens {
    pub index: usize,
    pub from: f64,
    pub to: f64,
}

impl gaanim_animation::AnimatableLens for CountLens {
    fn interpolate(&self, world: &mut World, entity: Entity, t: f64) {
        let count = self.from + (self.to - self.from) * t;
        let presence = Presence(presence(count, self.index));
        if world.get::<Presence>(entity) != Some(&presence) {
            world.entity_mut(entity).insert(presence);
        }
    }

    fn clone_box(&self) -> Box<dyn gaanim_animation::AnimatableLens> {
        Box::new(*self)
    }

    fn type_name(&self) -> &'static str {
        "Count"
    }

    fn hold_channel(&self) -> Option<&'static str> {
        Some("Count")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_animation::AnimatableLens;

    #[test]
    fn a_fractional_count_fades_only_the_next_copy() {
        assert_eq!(presence(2.5, 1), 1.0);
        assert_eq!(presence(2.5, 2), 0.5);
        assert_eq!(presence(2.5, 3), 0.0);
        assert_eq!(presence(0.0, 0), 0.0);

        let mut world = World::new();
        let copy = world.spawn_empty().id();
        let lens = CountLens {
            index: 3,
            from: 2.0,
            to: 6.0,
        };
        lens.interpolate(&mut world, copy, 0.375);
        assert_eq!(world.get::<Presence>(copy), Some(&Presence(0.5)));
        lens.interpolate(&mut world, copy, 1.0);
        assert_eq!(world.get::<Presence>(copy), Some(&Presence(1.0)));
    }
}
