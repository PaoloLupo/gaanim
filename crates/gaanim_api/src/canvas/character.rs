//! Characters in a scene: [`SceneModel::character`]. A character is the
//! audience's kind of character (see `gaanim_objects::character`), posed
//! every frame from the timeline's time: it breathes, blinks and plays the
//! expressions scheduled with [`CharacterHandle::express`].

use gaanim_objects::character::{Character, catalog, character_seed};
use gaanim_scene::StrokeBrush;

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::ops::{Op, SharedCanvasState};
use super::types::SpawnKind;

/// Errors raised while authoring a character.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CharacterError {
    #[error("character size must be positive, got {0}")]
    Size(f64),
    #[error(
        "a character is five indexes below [{bodies}, {colors}, {eyes}, {mouths}, {extras}] (body, color, eyes, mouth, extra), got {got:?}"
    )]
    Parts {
        got: Vec<usize>,
        bodies: usize,
        colors: usize,
        eyes: usize,
        mouths: usize,
        extras: usize,
    },
    #[error("unknown expression {name:?}; expressions: {known}")]
    Expression { name: String, known: String },
}

/// A character authored with [`SceneModel::character`].
#[derive(Clone)]
pub struct CharacterHandle {
    /// The group of its layers: place, scale and animate it like any
    /// drawable.
    pub group: DrawableHandle,
    character: Character,
    state: SharedCanvasState,
}

impl std::fmt::Debug for CharacterHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CharacterHandle")
            .field("character", &self.character)
            .finish_non_exhaustive()
    }
}

impl CharacterHandle {
    /// Its [body, color, eyes, mouth, extra].
    pub fn character(&self) -> Character {
        self.character
    }

    /// Play `expression` from the cursor, once or `looped` until the next
    /// one; `None` goes back to the character's own face.
    pub fn express(&self, expression: Option<&str>, looped: bool) -> Result<(), CharacterError> {
        if let Some(name) = expression
            && !catalog().expressions().contains(&name)
        {
            return Err(CharacterError::Expression {
                name: name.to_string(),
                known: catalog().expressions().join(", "),
            });
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::CharacterExpress {
                target: self.group.id,
                expression: expression.map(|name| (name.to_string(), looped)),
            });
        Ok(())
    }
}

impl SceneModel {
    /// A character at the origin, `size` units tall (its envelope, hats and
    /// ears included). `character` is [body, color, eyes, mouth, extra];
    /// without one, it is read from `name`, the nickname that also sets how
    /// it blinks.
    pub fn character(
        &mut self,
        character: Option<Character>,
        name: &str,
        size: f64,
    ) -> Result<CharacterHandle, CharacterError> {
        if !(size.is_finite() && size > 0.0) {
            return Err(CharacterError::Size(size));
        }
        let catalog = catalog();
        let seed = character_seed(name);
        let character = character.unwrap_or_else(|| catalog.character_from_seed(seed));
        if !catalog.contains(&character) {
            let [bodies, colors, eyes, mouths, extras] = catalog.counts();
            return Err(CharacterError::Parts {
                got: character.to_vec(),
                bodies,
                colors,
                eyes,
                mouths,
                extras,
            });
        }
        let envelope = gaanim_animation::characters::character_envelope(size);
        let bounds =
            gaanim_math::Bounds3D::new_2d(envelope.x0, envelope.y0, envelope.x1, envelope.y1);
        let rig = gaanim_animation::characters::CharacterRig {
            character,
            seed,
            size,
            layers: Vec::new(),
            schedule: Vec::new(),
        };
        // Each layer starts as the pose at time zero; the character system
        // redraws them every frame.
        let pose = gaanim_animation::characters::character_layers(&rig, 0.0);
        let layers: Vec<DrawableHandle> = (0..catalog.max_layers())
            .map(|index| {
                let (path, fill) = pose
                    .get(index)
                    .map_or((Default::default(), None), |(path, brush)| {
                        (path.clone(), Some(brush.clone()))
                    });
                super::canvas_impl::spawn_in(
                    &self.state,
                    SpawnKind::SvgPath(Box::new(gaanim_objects::prelude::SvgPath {
                        id: String::new(),
                        path,
                        bounds,
                        fill,
                        stroke: StrokeBrush::transparent(),
                    })),
                    false,
                )
            })
            .collect();
        let group = super::canvas_impl::spawn_in(
            &self.state,
            SpawnKind::Group(layers.iter().map(|layer| layer.id).collect()),
            true,
        );
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCharacter {
                target: group.id,
                layers: layers.iter().map(|layer| layer.id).collect(),
                rig,
            });
        Ok(CharacterHandle {
            group,
            character,
            state: self.state.clone(),
        })
    }
}
