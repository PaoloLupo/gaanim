//! Characters in a scene (`scene.character`): a group whose child paths are
//! the layers of a character's pose, recomputed every frame from the
//! timeline's time, so breathing, blinking and the expressions scheduled
//! with `character.express` seek and export exactly.

use std::sync::Arc;

use bevy::prelude::{Component, DetectChangesMut, Entity, Query, Res, Without};
use gaanim_core::kurbo::{BezPath, Rect};
use gaanim_core::peniko::Brush;
use gaanim_math::Bounds3D;
use gaanim_objects::character::{CHARACTER_ENVELOPE, Character, ExpressionPlay, catalog, to_scene};
use gaanim_scene::{FillBrush, LocalBounds, Path2D, PathSource};

use crate::updaters::PlaybackState;

/// The character catalog, for crates that reach it through animation.
pub use gaanim_objects::character::{Character as CharacterParts, catalog as character_catalog};

/// A character on the group entity that holds its layers.
#[derive(Component, Debug, Clone)]
pub struct CharacterRig {
    pub character: Character,
    /// Blinking seed, from the character's nickname.
    pub seed: u32,
    /// Height of the character's envelope in scene units.
    pub size: f64,
    /// The child paths, back to front; a pose uses the first ones and
    /// empties the rest.
    pub layers: Vec<Entity>,
    /// Expressions by the time they start: a name and whether it loops, or
    /// `None` to go back to the character's own face.
    pub schedule: Vec<(f64, Option<(String, bool)>)>,
}

impl CharacterRig {
    /// The expression playing at `time`, from the last one scheduled at or
    /// before it.
    pub fn expression_at(&self, time: f64) -> Option<ExpressionPlay> {
        let index = self
            .schedule
            .partition_point(|(start, _)| *start <= time + 1e-9);
        let (start, expression) = self.schedule.get(index.checked_sub(1)?)?;
        let (name, looped) = expression.as_ref()?;
        Some(ExpressionPlay {
            name: name.clone(),
            start: *start,
            looped: *looped,
        })
    }

    /// Schedule `expression` from `time`, keeping the schedule sorted.
    pub fn schedule(&mut self, time: f64, expression: Option<(String, bool)>) {
        let index = self.schedule.partition_point(|(start, _)| *start <= time);
        self.schedule.insert(index, (time, expression));
    }

    /// The layers' bounds: the envelope, whatever the pose.
    pub fn bounds(&self) -> Bounds3D {
        let rect = to_scene(self.size).transform_rect_bbox(CHARACTER_ENVELOPE);
        Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1)
    }
}

/// The paths and colors of `rig` at `time`, in scene units, back to front.
pub fn character_layers(rig: &CharacterRig, time: f64) -> Vec<(BezPath, Brush)> {
    let expression = rig.expression_at(time);
    let to_scene = to_scene(rig.size);
    catalog()
        .pose(&rig.character, rig.seed, time, expression.as_ref())
        .into_iter()
        .map(|layer| {
            let mut path = (*layer.outline).clone();
            path.apply_affine(to_scene * layer.transform);
            (path, Brush::Solid(layer.color))
        })
        .collect()
}

/// Draw each character's layers at the timeline's time, honoring a create
/// animation's reveal of each layer.
#[allow(clippy::type_complexity)]
pub fn character_system(
    playback: Option<Res<PlaybackState>>,
    rigs: Query<&CharacterRig>,
    mut layers: Query<
        (
            &mut Path2D,
            Option<&mut PathSource>,
            &mut FillBrush,
            &mut LocalBounds,
            Option<&crate::writing::PathReveal>,
        ),
        Without<CharacterRig>,
    >,
) {
    let time = playback.map_or(0.0, |playback| playback.current_time);
    for rig in &rigs {
        let pose = character_layers(rig, time);
        let bounds = rig.bounds();
        for (index, entity) in rig.layers.iter().enumerate() {
            let Ok((mut path, source, mut fill, mut local, reveal)) = layers.get_mut(*entity)
            else {
                continue;
            };
            let (outline, brush) = match pose.get(index) {
                Some((outline, brush)) => (Arc::new(outline.clone()), Some(brush.clone())),
                None => (Arc::new(BezPath::new()), None),
            };
            // Seeks restore paths and fills from a snapshot: write them
            // from the pose every frame.
            if let Some(mut source) = source {
                source.0 = outline.clone();
            }
            let reveal = reveal.map_or(1.0, |reveal| reveal.0);
            path.0 = crate::writing::path_at_reveal(&outline, reveal);
            if fill.0 != brush {
                fill.0 = brush;
            }
            local.set_if_neq(LocalBounds(bounds));
        }
    }
}

/// The envelope of a character `size` units tall, as a rectangle.
pub fn character_envelope(size: f64) -> Rect {
    to_scene(size).transform_rect_bbox(CHARACTER_ENVELOPE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig() -> CharacterRig {
        CharacterRig {
            character: [0, 0, 0, 1, 0],
            seed: 7,
            size: 2.0,
            layers: Vec::new(),
            schedule: Vec::new(),
        }
    }

    #[test]
    fn the_schedule_plays_the_last_expression_started() {
        let mut rig = rig();
        rig.schedule(2.0, None);
        rig.schedule(1.0, Some(("happy".into(), false)));
        rig.schedule(3.0, Some(("winner".into(), true)));
        assert_eq!(rig.expression_at(0.5), None);
        assert_eq!(rig.expression_at(1.5).unwrap().name, "happy");
        assert_eq!(rig.expression_at(2.5), None);
        let winner = rig.expression_at(9.0).unwrap();
        assert!(winner.looped && winner.start == 3.0);
    }

    #[test]
    fn layers_fit_the_envelope_centered_on_the_origin() {
        let rig = rig();
        let envelope = character_envelope(rig.size);
        assert!((envelope.height() - 2.0).abs() < 1e-9);
        assert!(envelope.center().x.abs() < 1e-9 && envelope.center().y.abs() < 1e-9);
        for (path, _) in character_layers(&rig, 0.0) {
            let inside = envelope.inflate(1e-6, 1e-6);
            let bbox = gaanim_core::kurbo::Shape::bounding_box(&path);
            assert!(inside.contains_rect(bbox), "{bbox:?} outside {envelope:?}");
        }
    }
}
