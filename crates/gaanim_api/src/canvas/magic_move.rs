//! Keyed "magic move" between two states of a drawable hierarchy.
//!
//! Drawables that carry the same key on both sides are paired: each pair
//! morphs position, size, color and shape, and every other drawable appears
//! or disappears. Keys are read while the scene is built, so the compiled
//! timeline only holds explicit pairs and stays seekable.

use std::collections::{HashMap, HashSet};

use gaanim_core::ObjectId;
pub use gaanim_timeline::transition::MagicMoveKey;

use super::types::{ObjectSpec, SpawnKind};

/// What happens to drawables that have no counterpart on the other side.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum MagicMoveUnmatched {
    /// Leaving drawables fade out and entering ones fade in over the move.
    #[default]
    Fade,
    /// Leaving drawables vanish when the move starts and entering ones
    /// appear when it ends.
    Cut,
}

impl MagicMoveUnmatched {
    /// Parse the Python spelling: `"fade"` or `"cut"`.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fade" => Some(Self::Fade),
            "cut" => Some(Self::Cut),
            _ => None,
        }
    }

    /// The Python spelling of this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::Cut => "cut",
        }
    }
}

/// Invalid arguments to a magic move.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MagicMoveError {
    #[error("magic_move drawables must belong to the same Scene")]
    ForeignScene,
    #[error("magic_move needs two different drawables")]
    SameDrawable,
    #[error("magic_move duration must be a finite positive number")]
    InvalidDuration,
}

/// Failure of a magic move whose keys come from a fallible callback.
#[derive(Debug)]
pub enum MagicMoveFailure<E> {
    /// The arguments were invalid.
    Invalid(MagicMoveError),
    /// The key callback failed.
    Key(E),
}

impl<E> From<MagicMoveError> for MagicMoveFailure<E> {
    fn from(error: MagicMoveError) -> Self {
        Self::Invalid(error)
    }
}

/// Result of pairing two keyed lists.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeyedPairs {
    /// `(source, target)` drawables that share a key.
    pub pairs: Vec<(ObjectId, ObjectId)>,
    /// Keyed sources without a counterpart.
    pub leaving: Vec<ObjectId>,
    /// Keyed targets without a counterpart.
    pub entering: Vec<ObjectId>,
}

/// Pair `sources` with `targets` by key.
///
/// The n-th source with a key pairs with the n-th target with the same key,
/// so repeated keys pair in declaration order. The pairs follow the order of
/// `sources`; unpaired drawables keep their own side's order.
pub fn pair_by_key(sources: &[(ObjectId, String)], targets: &[(ObjectId, String)]) -> KeyedPairs {
    let mut available: HashMap<&str, std::collections::VecDeque<ObjectId>> = HashMap::new();
    for (id, key) in targets {
        available.entry(key.as_str()).or_default().push_back(*id);
    }
    let mut result = KeyedPairs::default();
    let mut paired_targets = HashSet::new();
    for (source, key) in sources {
        match available
            .get_mut(key.as_str())
            .and_then(|queue| queue.pop_front())
        {
            Some(target) => {
                paired_targets.insert(target);
                result.pairs.push((*source, target));
            }
            None => result.leaving.push(*source),
        }
    }
    result.entering = targets
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| !paired_targets.contains(id))
        .collect();
    result
}

/// The key `spec` answers to, if any.
pub(crate) fn spec_key(spec: &ObjectSpec, key: MagicMoveKey) -> Option<String> {
    match key {
        MagicMoveKey::Name => spec.name.clone().or_else(|| spec.svg_id.clone()),
        MagicMoveKey::SvgId => spec.svg_id.clone(),
    }
}

/// Direct members of a group spec; other drawables have none.
pub(crate) fn spec_children(spec: &ObjectSpec) -> Vec<ObjectId> {
    match &spec.kind {
        SpawnKind::Group(members) | SpawnKind::GroupNoCenter(members) => members.clone(),
        _ => Vec::new(),
    }
}

/// The outermost keyed drawables reachable from `roots`, in depth-first
/// declaration order.
///
/// A keyed drawable moves as a whole, so drawables inside it are not
/// candidates even when they have keys of their own. `key` is evaluated once
/// per reachable drawable. With `include_roots` false the roots themselves are
/// never candidates, only their descendants.
pub(crate) fn keyed_outermost<E>(
    roots: &[ObjectId],
    include_roots: bool,
    children: impl Fn(ObjectId) -> Vec<ObjectId>,
    mut key: impl FnMut(ObjectId) -> Result<Option<String>, E>,
) -> Result<Vec<(ObjectId, String)>, E> {
    let root_set: HashSet<ObjectId> = roots.iter().copied().collect();
    let mut order = Vec::new();
    let mut keys: HashMap<ObjectId, Option<String>> = HashMap::new();
    let mut parents: HashMap<ObjectId, Vec<ObjectId>> = HashMap::new();
    let mut stack: Vec<ObjectId> = roots.iter().rev().copied().collect();
    while let Some(id) = stack.pop() {
        if keys.contains_key(&id) {
            continue;
        }
        let own_key = if include_roots || !root_set.contains(&id) {
            key(id)?.filter(|value| !value.is_empty())
        } else {
            None
        };
        keys.insert(id, own_key);
        order.push(id);
        let members = children(id);
        for member in members.iter().rev() {
            parents.entry(*member).or_default().push(id);
            stack.push(*member);
        }
    }

    let has_keyed_ancestor = |id: ObjectId| {
        let mut seen = HashSet::new();
        let mut pending = parents.get(&id).cloned().unwrap_or_default();
        while let Some(parent) = pending.pop() {
            if !seen.insert(parent) {
                continue;
            }
            if keys.get(&parent).is_some_and(Option::is_some) {
                return true;
            }
            pending.extend(parents.get(&parent).into_iter().flatten().copied());
        }
        false
    };
    Ok(order
        .iter()
        .filter_map(|id| {
            let key = keys.get(id)?.clone()?;
            (!has_keyed_ancestor(*id)).then_some((*id, key))
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(raw: u64) -> ObjectId {
        ObjectId::from_raw(raw)
    }

    fn keyed(items: &[(u64, &str)]) -> Vec<(ObjectId, String)> {
        items
            .iter()
            .map(|(raw, key)| (id(*raw), key.to_string()))
            .collect()
    }

    #[test]
    fn pairs_by_key_and_reports_unmatched_sides() {
        let result = pair_by_key(
            &keyed(&[(1, "alice"), (2, "bob"), (3, "carol")]),
            &keyed(&[(12, "bob"), (14, "dave"), (11, "alice")]),
        );
        assert_eq!(result.pairs, vec![(id(1), id(11)), (id(2), id(12))]);
        assert_eq!(result.leaving, vec![id(3)]);
        assert_eq!(result.entering, vec![id(14)]);
    }

    #[test]
    fn repeated_keys_pair_in_declaration_order() {
        let result = pair_by_key(
            &keyed(&[(1, "dot"), (2, "dot"), (3, "dot")]),
            &keyed(&[(11, "dot"), (12, "dot")]),
        );
        assert_eq!(result.pairs, vec![(id(1), id(11)), (id(2), id(12))]);
        assert_eq!(result.leaving, vec![id(3)]);
        assert!(result.entering.is_empty());
    }

    #[test]
    fn outermost_keyed_members_hide_their_keyed_descendants() {
        // 1 = root { 2 = "row" { 3 = "label" }, 4 = unkeyed { 5 = "badge" } }
        let children = |node: ObjectId| match node.as_raw() {
            1 => vec![id(2), id(4)],
            2 => vec![id(3)],
            4 => vec![id(5)],
            _ => Vec::new(),
        };
        let names: HashMap<u64, &str> = [(1, "root"), (2, "row"), (3, "label"), (5, "badge")]
            .into_iter()
            .collect();
        let mut calls = Vec::new();
        let found = keyed_outermost(&[id(1)], false, children, |node| {
            calls.push(node.as_raw());
            Ok::<_, ()>(names.get(&node.as_raw()).map(|name| name.to_string()))
        })
        .unwrap();
        assert_eq!(found, keyed(&[(2, "row"), (5, "badge")]));
        assert!(!calls.contains(&1), "roots are not candidates");
    }

    #[test]
    fn a_member_declared_before_its_keyed_group_stays_inside_it() {
        // Segment roots list the member (3) before its keyed group (2).
        let children = |node: ObjectId| match node.as_raw() {
            2 => vec![id(3)],
            _ => Vec::new(),
        };
        let found = keyed_outermost(&[id(3), id(2)], true, children, |node| {
            Ok::<_, ()>(Some(format!("k{}", node.as_raw())))
        })
        .unwrap();
        assert_eq!(found, keyed(&[(2, "k2")]));
    }

    #[test]
    fn key_errors_propagate_and_empty_keys_are_ignored() {
        let children = |node: ObjectId| match node.as_raw() {
            1 => vec![id(2), id(3)],
            _ => Vec::new(),
        };
        let found = keyed_outermost(&[id(1)], false, children, |node| {
            Ok::<_, ()>(Some(if node.as_raw() == 2 {
                String::new()
            } else {
                "x".to_string()
            }))
        })
        .unwrap();
        assert_eq!(found, keyed(&[(3, "x")]));
        let failed = keyed_outermost(&[id(1)], false, children, |_| {
            Err::<Option<String>, _>("boom")
        });
        assert_eq!(failed, Err("boom"));
    }

    #[test]
    fn unmatched_modes_parse_their_python_spelling() {
        assert_eq!(
            MagicMoveUnmatched::parse("fade"),
            Some(MagicMoveUnmatched::Fade)
        );
        assert_eq!(
            MagicMoveUnmatched::parse("CUT"),
            Some(MagicMoveUnmatched::Cut)
        );
        assert_eq!(MagicMoveUnmatched::parse("scale"), None);
    }
}
