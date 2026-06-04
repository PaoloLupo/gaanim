use gaanim_core::glam::DVec3;
use gaanim_math::{Bounds3D, SpatialTransform};
use crate::{Anchor, Direction};

/// Transforms a local bounding box into parent/world space by applying the spatial transform.
pub fn transform_bounds(bounds: Bounds3D, transform: &SpatialTransform) -> Bounds3D {
    let mat = transform.to_mat4();
    let z = bounds.min.z;
    let corners = [
        DVec3::new(bounds.min.x, bounds.min.y, z),
        DVec3::new(bounds.max.x, bounds.min.y, z),
        DVec3::new(bounds.min.x, bounds.max.y, z),
        DVec3::new(bounds.max.x, bounds.max.y, z),
    ];
    let mut min = DVec3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
    let mut max = DVec3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for corner in corners {
        let transformed = mat.transform_point3(corner);
        min = min.min(transformed);
        max = max.max(transformed);
    }
    Bounds3D::new(min, max)
}

/// Computes the translation needed to move an object so that its
/// specified anchor point lands at the target world-space position.
pub fn compute_move_to(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    target: DVec3,
    anchor: Anchor,
) -> SpatialTransform {
    let mut new_transform = *transform;
    let anchor_local = anchor.get_point(&bounds);
    
    // Find the world space offset of anchor_local relative to translation
    let pivot = transform.anchor;
    let offset_world = pivot + transform.rotation * (transform.scale * (anchor_local - pivot));
    
    new_transform.translation = target - offset_world;
    new_transform
}

/// Computes translation to place object at screen edge/corner
/// with buffer spacing.
pub fn compute_to_edge(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    direction: Direction,
    buff: f64,
    frame_bounds: Bounds3D,
) -> SpatialTransform {
    let dir_vec = direction.to_vector();
    let mut new_transform = *transform;
    let mut translation = transform.translation;

    let mut temp_transform = *transform;
    temp_transform.translation = DVec3::ZERO;
    let world_bounds = transform_bounds(bounds, &temp_transform);

    if dir_vec.x > 1e-5 {
        // Right
        translation.x = frame_bounds.max.x - buff - world_bounds.max.x;
    } else if dir_vec.x < -1e-5 {
        // Left
        translation.x = frame_bounds.min.x + buff - world_bounds.min.x;
    }

    if dir_vec.y > 1e-5 {
        // Up
        translation.y = frame_bounds.max.y - buff - world_bounds.max.y;
    } else if dir_vec.y < -1e-5 {
        // Down
        translation.y = frame_bounds.min.y + buff - world_bounds.min.y;
    }

    new_transform.translation = translation;
    new_transform
}

/// Computes translation to place object at screen corner
/// with buffer spacing (convenience for to_edge with diagonal).
pub fn compute_to_corner(
    bounds: Bounds3D,
    transform: &SpatialTransform,
    corner: Anchor,
    buff: f64,
    frame_bounds: Bounds3D,
) -> SpatialTransform {
    let direction = match corner {
        Anchor::TopLeft => Direction::UpLeft,
        Anchor::TopRight => Direction::UpRight,
        Anchor::BottomLeft => Direction::DownLeft,
        Anchor::BottomRight => Direction::DownRight,
        Anchor::Top => Direction::Up,
        Anchor::Bottom => Direction::Down,
        Anchor::Left => Direction::Left,
        Anchor::Right => Direction::Right,
        Anchor::Center => Direction::Custom(DVec3::ZERO),
    };
    compute_to_edge(bounds, transform, direction, buff, frame_bounds)
}

/// Enhanced next_to with anchor alignment support.
pub fn compute_next_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    direction: Direction,
    spacing: f64,
    aligned_edge: Anchor,
) -> DVec3 {
    let target_world_bounds = transform_bounds(target_bounds, target_transform);
    let ref_world_bounds = transform_bounds(ref_bounds, ref_transform);

    let dir_vec = direction.to_vector();
    let opp_dir_vec = -dir_vec;

    let ref_anchor = Anchor::from_direction(dir_vec);
    let target_anchor = Anchor::from_direction(opp_dir_vec);

    let p_ref_boundary = ref_anchor.get_point(&ref_world_bounds);
    let p_target_boundary = target_anchor.get_point(&target_world_bounds);

    let p_ref_ortho = aligned_edge.get_point(&ref_world_bounds);
    let p_target_ortho = aligned_edge.get_point(&target_world_bounds);

    let mut shift = DVec3::ZERO;

    // X axis
    if dir_vec.x.abs() > 1e-5 {
        shift.x = p_ref_boundary.x + dir_vec.x * spacing - p_target_boundary.x;
    } else {
        shift.x = p_ref_ortho.x - p_target_ortho.x;
    }

    // Y axis
    if dir_vec.y.abs() > 1e-5 {
        shift.y = p_ref_boundary.y + dir_vec.y * spacing - p_target_boundary.y;
    } else {
        shift.y = p_ref_ortho.y - p_target_ortho.y;
    }

    // Z axis
    if dir_vec.z.abs() > 1e-5 {
        shift.z = p_ref_boundary.z + dir_vec.z * spacing - p_target_boundary.z;
    } else {
        shift.z = p_ref_ortho.z - p_target_ortho.z;
    }

    shift
}

/// Enhanced align_to: align a specific anchor of target with a specific anchor of reference.
pub fn compute_align_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    target_anchor: Anchor,
    ref_anchor: Anchor,
) -> DVec3 {
    let target_world_bounds = transform_bounds(target_bounds, target_transform);
    let ref_world_bounds = transform_bounds(ref_bounds, ref_transform);

    let p_ref = ref_anchor.get_point(&ref_world_bounds);
    let p_target = target_anchor.get_point(&target_world_bounds);

    p_ref - p_target
}
