use gaanim_core::glam::DVec3;
use gaanim_math::{Bounds3D, SpatialTransform};
use crate::{Anchor, Direction};
use crate::positioning::transform_bounds;

/// Returns the world-space position of an anchor point on an object.
pub fn get_anchor_point(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    anchor: Anchor,
) -> DVec3 {
    let world_bounds = transform_bounds(bounds, transform);
    anchor.get_point(&world_bounds)
}

/// Shorthand for get_anchor_point(Center)
pub fn get_center(bounds: Bounds3D, transform: &SpatialTransform) -> DVec3 {
    get_anchor_point(bounds, transform, Anchor::Center)
}

/// Shorthand for get_anchor_point with corner anchors (e.g. TopLeft, TopRight, etc.)
pub fn get_corner(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    corner: Anchor,
) -> DVec3 {
    get_anchor_point(bounds, transform, corner)
}

/// Returns the world-space position of an edge center.
pub fn get_edge_center(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    edge: Direction,
) -> DVec3 {
    get_anchor_point(bounds, transform, edge.to_anchor())
}

/// Returns the width of the world-space bounds.
pub fn get_width(bounds: Bounds3D, transform: &SpatialTransform) -> f64 {
    transform_bounds(bounds, transform).size().x
}

/// Returns the height of the world-space bounds.
pub fn get_height(bounds: Bounds3D, transform: &SpatialTransform) -> f64 {
    transform_bounds(bounds, transform).size().y
}
