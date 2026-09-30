//! Followers that copy a leader's position with a delay (trails and tails).

use bevy::prelude::{Component, Entity};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DMat4, DVec3};

use crate::FollowOffsetSpace;

/// Keeps a drawable where the Mobject `source` was `delay` seconds earlier,
/// plus `offset` (After Effects' `valueAtTime(time - delay)`).
///
/// The timeline evaluates `source` at `t - delay` from its keyframe and
/// clips after every seek, exactly as a seek rebuilds it, so a follower
/// needs no history and any seek reproduces the trail. When `source` is
/// itself a follower its position is rebuilt at that time too, so followers
/// chain. Until the current
/// segment has run for `delay` seconds the follower holds the leader's
/// position at the segment start.
#[derive(Component, Debug, Clone, PartialEq)]
pub struct DelayedFollow {
    /// Runtime identity of the followed Mobject.
    pub source: ObjectId,
    /// How far back, in timeline seconds, the follower reads `source`.
    pub delay: f64,
    /// Displacement from the leader's delayed position.
    pub offset: DVec3,
    /// Whether `offset` is in scene axes or the leader's delayed axes
    /// (rotated and scaled with it).
    pub offset_space: FollowOffsetSpace,
    /// Scratch entity the timeline evaluates the leader into.
    pub probe: Option<Entity>,
}

impl DelayedFollow {
    pub fn new(
        source: ObjectId,
        delay: f64,
        offset: DVec3,
        offset_space: FollowOffsetSpace,
    ) -> Self {
        Self {
            source,
            delay: delay.max(0.0),
            offset,
            offset_space,
            probe: None,
        }
    }

    /// The timeline time whose leader state the follower shows at `time`,
    /// never earlier than `start` (the current segment's start).
    pub fn source_time(&self, time: f64, start: f64) -> f64 {
        (time - self.delay).max(start)
    }

    /// World position of the follower given the leader's world matrix at
    /// the delayed time.
    pub fn position(&self, source_world: DMat4) -> DVec3 {
        let origin = source_world.transform_point3(DVec3::ZERO);
        let offset = match self.offset_space {
            FollowOffsetSpace::World => self.offset,
            FollowOffsetSpace::Local => source_world.transform_vector3(self.offset),
        };
        origin + offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_core::glam::DQuat;

    #[test]
    fn source_time_lags_and_holds_at_the_segment_start() {
        let follow = DelayedFollow::new(
            ObjectId::from_raw(1),
            0.25,
            DVec3::ZERO,
            FollowOffsetSpace::World,
        );
        assert_eq!(follow.source_time(1.0, 0.0), 0.75);
        assert_eq!(follow.source_time(0.1, 0.0), 0.0);
        assert_eq!(follow.source_time(2.1, 2.0), 2.0);
        // A negative delay is treated as none.
        let now = DelayedFollow::new(
            ObjectId::from_raw(1),
            -1.0,
            DVec3::ZERO,
            FollowOffsetSpace::World,
        );
        assert_eq!(now.source_time(1.0, 0.0), 1.0);
    }

    #[test]
    fn offset_follows_world_or_leader_axes() {
        let leader = DMat4::from_rotation_translation(
            DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
            DVec3::new(1.0, 2.0, 0.0),
        );
        let world = DelayedFollow::new(
            ObjectId::from_raw(1),
            0.1,
            DVec3::new(1.0, 0.0, 0.0),
            FollowOffsetSpace::World,
        );
        assert!((world.position(leader) - DVec3::new(2.0, 2.0, 0.0)).length() < 1e-12);
        let local = DelayedFollow {
            offset_space: FollowOffsetSpace::Local,
            ..world
        };
        assert!((local.position(leader) - DVec3::new(1.0, 3.0, 0.0)).length() < 1e-12);
    }
}
