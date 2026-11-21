//! Drawables that link somewhere: a click on one while presenting jumps to
//! another segment or opens a web address (`drawable.link_to(...)`).
//!
//! A link's [`LinkZone`] is the box the drawable covers in the camera's
//! frame, 0 to 1 across it with y down, so a presentation, a playback bundle
//! and a PDF page can each find it without the scene's entities.

use bevy::prelude::*;
use gaanim_math::{Bounds3D, Camera};

use crate::{GlobalOpacity, Path2D, Visible, WorldBounds};

/// Where a link goes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", content = "to", rename_all = "lowercase")
)]
pub enum LinkTarget {
    /// The segment with this id.
    Segment(u32),
    /// A web address.
    Url(String),
}

/// A drawable that links to `0`.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub struct Hyperlink(pub LinkTarget);

/// The box a link covers in the camera's frame, 0 to 1 across it with y
/// down: `[left, top, right, bottom]`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LinkZone {
    pub target: LinkTarget,
    pub rect: [f32; 4],
}

impl LinkZone {
    /// Whether `point` (frame coordinates, 0 to 1, y down) is inside.
    pub fn contains(&self, [x, y]: [f32; 2]) -> bool {
        let [left, top, right, bottom] = self.rect;
        (left..=right).contains(&x) && (top..=bottom).contains(&y)
    }
}

/// The link under `point`, the smallest when several cover it.
pub fn link_at(zones: &[LinkZone], point: [f32; 2]) -> Option<&LinkZone> {
    zones
        .iter()
        .filter(|zone| zone.contains(point))
        .min_by(|left, right| area(left).total_cmp(&area(right)))
}

fn area(zone: &LinkZone) -> f32 {
    let [left, top, right, bottom] = zone.rect;
    (right - left) * (bottom - top)
}

/// Entities a link's box looks at, at most, so a huge group costs little.
const MAX_ENTITIES: usize = 10_000;

/// The zones of the visible links in `world`, seen through `camera`.
pub fn link_zones(world: &mut World, camera: &Camera) -> Vec<LinkZone> {
    let mut links = world.query::<(Entity, &Hyperlink)>();
    let linked: Vec<(Entity, LinkTarget)> = links
        .iter(world)
        .map(|(entity, link)| (entity, link.0.clone()))
        .collect();
    if linked.is_empty() {
        return Vec::new();
    }
    let mut boxes = world.query::<(
        Option<&WorldBounds>,
        Option<&Path2D>,
        Option<&gaanim_math::GlobalSpatialTransform>,
        Has<Visible>,
        Option<&GlobalOpacity>,
        Option<&Children>,
    )>();
    let (width, height) = (
        f64::from(camera.viewport_width.max(1)),
        f64::from(camera.viewport_height.max(1)),
    );
    linked
        .into_iter()
        .filter_map(|(entity, target)| {
            let mut pending = vec![entity];
            let mut seen = 0;
            let mut covered: Option<Bounds3D> = None;
            while let Some(current) = pending.pop() {
                seen += 1;
                if seen > MAX_ENTITIES {
                    break;
                }
                let Ok((bounds, path, global, visible, opacity, children)) =
                    boxes.get(world, current)
                else {
                    continue;
                };
                if let Some(children) = children {
                    pending.extend(children.iter());
                }
                if !visible || opacity.is_some_and(|opacity| opacity.0 <= 0.001) {
                    continue;
                }
                let found = bounds.map(|bounds| bounds.0).or_else(|| {
                    let rect = global?.affine_2d.transform_rect_bbox(
                        gaanim_core::kurbo::Shape::bounding_box(path?.0.as_ref()),
                    );
                    [rect.x0, rect.y0, rect.x1, rect.y1]
                        .iter()
                        .all(|value| value.is_finite())
                        .then(|| Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1))
                });
                if let Some(found) = found
                    && (found.max - found.min).max_element() > 1e-9
                {
                    covered = Some(covered.map_or(found, |covered| covered.union(&found)));
                }
            }
            let covered = covered?;
            let corners = [
                (covered.min.x, covered.min.y),
                (covered.max.x, covered.min.y),
                (covered.min.x, covered.max.y),
                (covered.max.x, covered.max.y),
            ]
            .map(|(x, y)| camera.world_to_screen(glam::DVec3::new(x, y, 0.0)));
            let xs = corners.map(|corner| corner.x / width);
            let ys = corners.map(|corner| corner.y / height);
            let rect = [
                xs.into_iter().fold(f64::INFINITY, f64::min),
                ys.into_iter().fold(f64::INFINITY, f64::min),
                xs.into_iter().fold(f64::NEG_INFINITY, f64::max),
                ys.into_iter().fold(f64::NEG_INFINITY, f64::max),
            ]
            .map(|value| value as f32);
            let on_screen = rect.iter().all(|value| value.is_finite())
                && rect[2] > 0.0
                && rect[3] > 0.0
                && rect[0] < 1.0
                && rect[1] < 1.0;
            on_screen.then_some(LinkZone { target, rect })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone(rect: [f32; 4], to: &str) -> LinkZone {
        LinkZone {
            target: LinkTarget::Url(to.to_string()),
            rect,
        }
    }

    #[test]
    fn the_smallest_zone_under_the_point_wins() {
        let zones = [
            zone([0.0, 0.0, 1.0, 1.0], "card"),
            zone([0.4, 0.4, 0.6, 0.6], "chip"),
        ];
        assert_eq!(link_at(&zones, [0.5, 0.5]), Some(&zones[1]));
        assert_eq!(link_at(&zones, [0.1, 0.9]), Some(&zones[0]));
        assert_eq!(link_at(&zones[1..], [0.1, 0.9]), None);
    }

    #[test]
    fn visible_links_are_boxed_in_the_frame() {
        let mut world = World::new();
        let camera = Camera::ortho_2d_frame(16.0, 9.0, 1600, 900);
        world.spawn((
            Hyperlink(LinkTarget::Segment(3)),
            WorldBounds(Bounds3D::new_2d(-8.0, 0.0, 0.0, 4.5)),
            Visible,
        ));
        // Hidden links have no zone.
        world.spawn((
            Hyperlink(LinkTarget::Url("https://example.org".into())),
            WorldBounds(Bounds3D::new_2d(0.0, 0.0, 1.0, 1.0)),
        ));
        let zones = link_zones(&mut world, &camera);
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].target, LinkTarget::Segment(3));
        let [left, top, right, bottom] = zones[0].rect;
        assert!(left.abs() < 1e-4 && top.abs() < 1e-4, "{:?}", zones[0].rect);
        assert!((right - 0.5).abs() < 1e-4 && (bottom - 0.5).abs() < 1e-4);
    }
}
