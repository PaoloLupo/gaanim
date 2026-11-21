//! Links and backup segments: a drawable linked to a segment or a web
//! address (`drawable.link_to(...)`) jumps there when clicked while
//! presenting, and a backup segment (`skip=True`) is visited only that way.

use gaanim_core::ObjectId;
use gaanim_scene::links::LinkTarget;

use super::SceneModel;
use super::drawable::DrawableHandle;
use super::segment::{SegmentError, SegmentHandle, SegmentId};

/// Where a drawable links, as authored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkSpec {
    Segment(SegmentId),
    /// A segment named in a string, found when the scene compiles.
    SegmentName(String),
    Url(String),
}

impl LinkSpec {
    /// A string target: a web address when it has a scheme (`https://`,
    /// `mailto:`), else the name of a segment.
    pub fn parse(target: &str) -> Result<Self, LinkError> {
        let target = target.trim();
        if target.is_empty() {
            return Err(LinkError::EmptyTarget);
        }
        let scheme = target.split_once(':').is_some_and(|(scheme, rest)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && (rest.starts_with("//") || matches!(scheme, "mailto" | "tel"))
        });
        Ok(if scheme {
            Self::Url(target.to_string())
        } else {
            Self::SegmentName(target.to_string())
        })
    }
}

/// Why a link cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    #[error("a link needs a segment or a web address")]
    EmptyTarget,
    #[error("the segment belongs to another scene")]
    ForeignSegment,
}

impl DrawableHandle {
    /// Link this drawable to `target`; a later link replaces it.
    pub fn link_to(&self, target: LinkSpec) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        state.links.retain(|(id, _)| *id != self.id);
        state.links.push((self.id, target));
    }

    /// Link this drawable to `segment`.
    pub fn link_to_segment(&self, segment: &SegmentHandle) -> Result<(), LinkError> {
        if !segment.belongs_to(&self.state) {
            return Err(LinkError::ForeignSegment);
        }
        self.link_to(LinkSpec::Segment(segment.id()));
        Ok(())
    }

    /// Where this drawable links, if anywhere.
    pub fn link(&self) -> Option<LinkSpec> {
        let state = self.state.lock().expect("canvas state poisoned");
        state
            .links
            .iter()
            .find(|(id, _)| *id == self.id)
            .map(|(_, target)| target.clone())
    }
}

impl SceneModel {
    /// Make `segment` a backup or not: a presentation passes over a backup,
    /// which only a link visits.
    pub fn set_segment_backup(
        &mut self,
        segment: &SegmentHandle,
        backup: bool,
    ) -> Result<(), SegmentError> {
        if !segment.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        state.backup_segments.retain(|id| *id != segment.id());
        if backup {
            state.backup_segments.push(segment.id());
        }
        Ok(())
    }

    /// Ids of the backup segments.
    pub fn backup_segment_ids(&self) -> Vec<u32> {
        let state = self.state.lock().expect("canvas state poisoned");
        state.backup_segments.iter().map(|id| id.raw()).collect()
    }

    /// The scene's links, with segments named in strings found; a link to a
    /// segment that does not exist is left out (see [`Self::link_problems`]).
    pub fn resolved_links(&self) -> Vec<(ObjectId, LinkTarget)> {
        let state = self.state.lock().expect("canvas state poisoned");
        state
            .links
            .iter()
            .filter_map(|(id, target)| {
                let target = match target {
                    LinkSpec::Segment(segment) => LinkTarget::Segment(segment.raw()),
                    LinkSpec::Url(url) => LinkTarget::Url(url.clone()),
                    LinkSpec::SegmentName(name) => {
                        let wanted = name.to_lowercase();
                        let segment = state.segments.iter().find(|segment| {
                            segment.explicit && segment.name.to_lowercase() == wanted
                        })?;
                        LinkTarget::Segment(segment.id.raw())
                    }
                };
                Some((*id, target))
            })
            .collect()
    }

    /// What `gaanim check` reports about links: names of segments that do
    /// not exist.
    pub fn link_problems(&self) -> Vec<String> {
        let state = self.state.lock().expect("canvas state poisoned");
        state
            .links
            .iter()
            .filter_map(|(_, target)| match target {
                LinkSpec::SegmentName(name)
                    if !state.segments.iter().any(|segment| {
                        segment.explicit && segment.name.to_lowercase() == name.to_lowercase()
                    }) =>
                {
                    Some(format!(
                        "a link goes to segment `{name}`, which does not exist"
                    ))
                }
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_targets_are_addresses_with_a_scheme_or_segment_names() {
        assert_eq!(
            LinkSpec::parse(" https://doi.org/10.1000/1 "),
            Ok(LinkSpec::Url("https://doi.org/10.1000/1".into()))
        );
        assert_eq!(
            LinkSpec::parse("mailto:ana@example.org"),
            Ok(LinkSpec::Url("mailto:ana@example.org".into()))
        );
        assert_eq!(
            LinkSpec::parse("Results: details"),
            Ok(LinkSpec::SegmentName("Results: details".into()))
        );
        assert_eq!(LinkSpec::parse("  "), Err(LinkError::EmptyTarget));
    }

    #[test]
    fn links_resolve_their_segments_and_report_missing_ones() {
        let mut scene = SceneModel::new(16, 9);
        scene.segment("Intro", None).unwrap();
        let chip = scene.rect(1.0, 1.0);
        let backup = scene.segment("Backup", None).unwrap();
        scene.set_segment_backup(&backup, true).unwrap();
        let reference = scene.rect(1.0, 1.0);
        let lost = scene.rect(1.0, 1.0);
        chip.link_to_segment(&backup).unwrap();
        reference.link_to(LinkSpec::parse("https://example.org").unwrap());
        lost.link_to(LinkSpec::parse("Nowhere").unwrap());
        assert_eq!(scene.backup_segment_ids(), [backup.id().raw()]);
        let resolved = scene.resolved_links();
        assert_eq!(resolved.len(), 2);
        assert_eq!(
            resolved[0],
            (chip.id, LinkTarget::Segment(backup.id().raw()))
        );
        assert_eq!(scene.link_problems().len(), 1);
        // A later link replaces the first.
        chip.link_to(LinkSpec::parse("intro").unwrap());
        assert_eq!(chip.link(), Some(LinkSpec::SegmentName("intro".into())));
        assert!(scene.link_problems().len() == 1);
    }
}

#[cfg(test)]
mod compile_tests {
    use super::*;
    use bevy::prelude::World;

    #[test]
    fn compiled_links_mark_their_drawables() {
        let mut scene = SceneModel::new(16, 9);
        scene.segment("Intro", None).unwrap();
        let chip = scene.rect(2.0, 1.0);
        let backup = scene.segment("Backup", None).unwrap();
        chip.link_to_segment(&backup).unwrap();
        let mut world = World::new();
        let mut timeline = gaanim_timeline::timeline::Timeline::new();
        let fonts = gaanim_text::font::FontRegistry::new();
        let text_config = gaanim_text::prelude::TextConfig::default();
        {
            let mut commands = world.commands();
            scene.compile_into(&mut commands, &mut timeline, &fonts, &text_config);
        }
        world.flush();
        let mut links = world.query::<&gaanim_scene::links::Hyperlink>();
        assert_eq!(links.iter(&world).count(), 1, "one linked entity");
    }
}
