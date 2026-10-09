//! What the editor's inspector shows about the drawables a script authored:
//! their kind, name and the script line that created them.

use std::collections::HashMap;
use std::fmt::Write as _;

use gaanim_core::ObjectId;
use gaanim_scene::{AuthoredObject, AuthoredObjects};

use super::SceneModel;
use super::types::SpawnKind;

impl SceneModel {
    /// Every drawable of the scene as its script authored it, by its
    /// authored id.
    pub(crate) fn authored_objects(&self) -> HashMap<ObjectId, AuthoredObject> {
        let state = self.state.lock().expect("canvas state poisoned");
        state
            .object_specs
            .iter()
            .map(|(&id, spec)| {
                let spec = spec.lock().expect("object spec poisoned");
                let mut stack = state
                    .object_locations
                    .get(&id)
                    .cloned()
                    .unwrap_or_default()
                    .into_iter();
                let object = AuthoredObject {
                    kind: kind_name(&spec.kind),
                    name: spec.name.clone(),
                    location: stack.next(),
                    callers: stack.collect(),
                };
                (id, object)
            })
            .collect()
    }
}

/// The authored drawables keyed by the objects compiled for them; `ids`
/// maps authored ids to compiled ones, and an id it lacks compiled as itself.
pub(crate) fn compiled_index(
    authored: HashMap<ObjectId, AuthoredObject>,
    ids: Option<&HashMap<ObjectId, ObjectId>>,
) -> AuthoredObjects {
    AuthoredObjects(
        authored
            .into_iter()
            .map(|(id, object)| {
                let compiled = ids.and_then(|ids| ids.get(&id)).copied().unwrap_or(id);
                (compiled, object)
            })
            .collect(),
    )
}

/// The variant name of `kind` in snake case, as the scripting API spells
/// drawables (`RoundedRect` is `rounded_rect`).
pub(crate) fn kind_name(kind: &SpawnKind) -> String {
    /// Keeps what `Debug` writes up to the first character that cannot be
    /// part of a name, then stops it: a variant's data can be large.
    struct Head(String);
    impl std::fmt::Write for Head {
        fn write_str(&mut self, text: &str) -> std::fmt::Result {
            for ch in text.chars() {
                if !(ch.is_alphanumeric() || ch == '_') {
                    return Err(std::fmt::Error);
                }
                self.0.push(ch);
            }
            Ok(())
        }
    }
    let mut head = Head(String::new());
    let _ = write!(head, "{kind:?}");
    let mut name = String::with_capacity(head.0.len() + 4);
    for (index, ch) in head.0.chars().enumerate() {
        if ch.is_uppercase() {
            if index > 0 {
                name.push('_');
            }
            name.extend(ch.to_lowercase());
        } else {
            name.push(ch);
        }
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_named_like_the_scripting_api() {
        assert_eq!(kind_name(&SpawnKind::Circle(1.0)), "circle");
        assert_eq!(
            kind_name(&SpawnKind::RoundedRect(1.0, 2.0, 0.1)),
            "rounded_rect"
        );
        assert_eq!(kind_name(&SpawnKind::Group(Vec::new())), "group");
        assert_eq!(kind_name(&SpawnKind::SurroundingRect), "surrounding_rect");
    }

    #[test]
    fn the_index_follows_the_compiled_ids() {
        let object = AuthoredObject {
            kind: "circle".to_owned(),
            name: None,
            location: None,
            callers: Vec::new(),
        };
        let authored = HashMap::from([
            (ObjectId::from_raw(1), object.clone()),
            (ObjectId::from_raw(2), object.clone()),
        ]);
        let ids = HashMap::from([(ObjectId::from_raw(1), ObjectId::from_raw(7))]);
        let index = compiled_index(authored, Some(&ids));
        assert!(index.0.contains_key(&ObjectId::from_raw(7)));
        assert!(index.0.contains_key(&ObjectId::from_raw(2)));
        assert!(!index.0.contains_key(&ObjectId::from_raw(1)));
    }

    #[test]
    fn spawning_records_the_script_line_through_the_provider() {
        let mut canvas = SceneModel::new(640, 360);
        let circle = canvas.circle(1.0);
        let authored = canvas.authored_objects();
        let object = &authored[&circle.id];
        assert_eq!(object.kind, "circle");
        // No binding provides script lines in Rust tests.
        assert_eq!(object.location, None);
        assert!(object.callers.is_empty());
    }
}
