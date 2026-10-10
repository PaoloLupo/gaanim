//! What the editor's inspector shows about the drawables a script authored:
//! their kind, name and the script line that created them.

use std::collections::HashMap;

use gaanim_core::ObjectId;
use gaanim_scene::{AuthoredCall, AuthoredIndex, AuthoredObject, AuthoredObjects};

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
pub(crate) fn compiled_objects(
    authored: HashMap<ObjectId, AuthoredObject>,
    ids: &HashMap<ObjectId, ObjectId>,
) -> HashMap<ObjectId, AuthoredObject> {
    authored
        .into_iter()
        .map(|(id, object)| (ids.get(&id).copied().unwrap_or(id), object))
        .collect()
}

impl SceneModel {
    /// The script lines of each `play`, by the origin key its clips carry.
    pub(crate) fn authored_plays(&self) -> HashMap<u64, AuthoredCall> {
        let state = self.state.lock().expect("canvas state poisoned");
        state
            .op_locations
            .iter()
            .filter_map(|(&origin, stack)| {
                let mut stack = stack.iter().cloned();
                Some((
                    origin,
                    AuthoredCall {
                        location: stack.next()?,
                        callers: stack.collect(),
                    },
                ))
            })
            .collect()
    }
}

/// The index of `canvas`, built when the editor first reads it. `ids` holds
/// the authored ids that compiled under another id.
pub(crate) fn deferred_index(
    canvas: SceneModel,
    ids: HashMap<ObjectId, ObjectId>,
) -> AuthoredObjects {
    AuthoredObjects::deferred(move || AuthoredIndex {
        objects: compiled_objects(canvas.authored_objects(), &ids),
        plays: canvas.authored_plays(),
    })
}

/// The variant name of `kind` in snake case, as the scripting API spells
/// drawables (`RoundedRect` is `rounded_rect`).
pub(crate) fn kind_name(kind: &SpawnKind) -> String {
    gaanim_core::names::variant_name(kind)
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
        let index = compiled_objects(authored, &ids);
        assert!(index.contains_key(&ObjectId::from_raw(7)));
        assert!(index.contains_key(&ObjectId::from_raw(2)));
        assert!(!index.contains_key(&ObjectId::from_raw(1)));
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
