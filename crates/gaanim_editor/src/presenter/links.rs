//! Links while presenting: a click on a linked drawable jumps to its
//! segment, which plays and then returns to where the click was, or opens
//! its web address. A script finds its links on the scene's drawables; a
//! playback bundle reads where it recorded them at each resting instant.

use bevy::prelude::*;
use gaanim_scene::links::{LinkTarget, LinkZone};
use gaanim_timeline::timeline::Timeline;

use crate::{PresentationMode, ViewportFrame};

/// The links on screen this frame, in frame coordinates before any zoom.
#[derive(Resource, Debug, Default)]
pub(crate) struct CurrentLinks(pub Vec<LinkZone>);

/// Find the links on screen while presenting.
pub(crate) fn current_links_system(world: &mut World) {
    let presenting = world
        .get_resource::<PresentationMode>()
        .is_some_and(|mode| mode.active);
    let zones = if !presenting {
        Vec::new()
    } else if let Some(recorded) = world
        .get_resource::<Timeline>()
        .filter(|timeline| !timeline.link_zones.is_empty())
    {
        // A bundle knows its links where the presentation rests.
        if recorded.is_playing {
            Vec::new()
        } else {
            recorded
                .recorded_link_zones(recorded.current_time)
                .map(<[LinkZone]>::to_vec)
                .unwrap_or_default()
        }
    } else {
        let camera = world
            .get_resource::<gaanim_math::ResolvedCamera>()
            .map(|resolved| resolved.camera);
        match camera {
            Some(camera) => gaanim_scene::links::link_zones(world, &camera),
            None => Vec::new(),
        }
    };
    if let Some(mut current) = world.get_resource_mut::<CurrentLinks>() {
        if current.0 != zones {
            current.0 = zones;
        }
    } else {
        world.insert_resource(CurrentLinks(zones));
    }
}

/// The link under the audience window's `cursor`, if any: the frame is
/// where the slide shows, and the zoom of `Z`, if on, magnifies it.
pub(crate) fn link_under<'a>(
    links: &'a CurrentLinks,
    frame: &ViewportFrame,
    zoom: Option<super::ink::Zoom>,
    cursor: Vec2,
) -> Option<&'a LinkZone> {
    let shown = [
        (cursor.x - frame.origin.x as f32) / (frame.size.x as f32).max(1.0),
        (cursor.y - frame.origin.y as f32) / (frame.size.y as f32).max(1.0),
    ];
    let point = zoom.map_or(shown, |zoom| zoom.unproject(shown));
    gaanim_scene::links::link_at(&links.0, point)
}

/// Follow `link`: visit its segment, or open its address.
pub(crate) fn follow(link: &LinkZone, timeline: &mut Timeline) {
    match &link.target {
        LinkTarget::Segment(id) => {
            timeline.visit_segment(*id);
        }
        LinkTarget::Url(url) => {
            if let Err(error) = crate::platform::open_detached(url) {
                gaanim_core::console::warn("link", format!("could not open {url}: {error}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_finds_the_link_under_it_through_the_zoom() {
        let links = CurrentLinks(vec![LinkZone {
            target: LinkTarget::Segment(2),
            rect: [0.5, 0.5, 0.6, 0.6],
        }]);
        let frame = ViewportFrame {
            origin: glam::DVec2::new(100.0, 0.0),
            size: glam::DVec2::new(1000.0, 500.0),
            output_size: glam::DVec2::new(1600.0, 800.0),
        };
        assert!(link_under(&links, &frame, None, Vec2::new(650.0, 280.0)).is_some());
        assert!(link_under(&links, &frame, None, Vec2::new(150.0, 280.0)).is_none());
        // Zoomed in 2x around the slide's center, the link shows twice as
        // far from it.
        let zoom = super::super::ink::Zoom {
            focus: [0.5, 0.5],
            factor: 2.0,
        };
        assert!(link_under(&links, &frame, Some(zoom), Vec2::new(650.0, 280.0)).is_some());
        assert!(link_under(&links, &frame, Some(zoom), Vec2::new(750.0, 330.0)).is_some());
        assert!(link_under(&links, &frame, Some(zoom), Vec2::new(850.0, 380.0)).is_none());
    }

    #[test]
    fn following_a_segment_link_visits_it() {
        let mut timeline = Timeline::default();
        timeline.set_segments(vec![gaanim_timeline::timeline::SegmentMetadata {
            id: 2,
            name: "Backup".into(),
            notes: None,
            target: None,
            start_time: 4.0,
            end_time: 6.0,
            stops: Vec::new(),
        }]);
        follow(
            &LinkZone {
                target: LinkTarget::Segment(2),
                rect: [0.0; 4],
            },
            &mut timeline,
        );
        assert_eq!(timeline.visit.map(|visit| visit.segment), Some(2));
        assert_eq!(timeline.seek_request, Some(4.0));
    }
}
