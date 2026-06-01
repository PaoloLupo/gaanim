use gaanim_core::glam::DVec3;
use gaanim_math::{Bounds3D, SpatialTransform};

/// Standard layout anchors representing discrete alignment points on a bounding box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LayoutAnchor {
    Center,
    Left,
    Right,
    Top,
    Bottom,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl LayoutAnchor {
    /// Computes the 3D point location of this anchor relative to a given bounding box.
    pub fn get_point(&self, bounds: &Bounds3D) -> DVec3 {
        let center = bounds.center();
        match self {
            Self::Center => center,
            Self::Left => DVec3::new(bounds.min.x, center.y, center.z),
            Self::Right => DVec3::new(bounds.max.x, center.y, center.z),
            Self::Top => DVec3::new(center.x, bounds.max.y, center.z),
            Self::Bottom => DVec3::new(center.x, bounds.min.y, center.z),
            Self::TopLeft => DVec3::new(bounds.min.x, bounds.max.y, center.z),
            Self::TopRight => DVec3::new(bounds.max.x, bounds.max.y, center.z),
            Self::BottomLeft => DVec3::new(bounds.min.x, bounds.min.y, center.z),
            Self::BottomRight => DVec3::new(bounds.max.x, bounds.min.y, center.z),
        }
    }
}

/// Standard layout directions.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum LayoutDirection {
    Up,
    Down,
    Left,
    Right,
    Custom(DVec3),
}

impl LayoutDirection {
    /// Returns the unit direction vector.
    pub fn to_vector(&self) -> DVec3 {
        match self {
            Self::Up => DVec3::new(0.0, 1.0, 0.0),
            Self::Down => DVec3::new(0.0, -1.0, 0.0),
            Self::Left => DVec3::new(-1.0, 0.0, 0.0),
            Self::Right => DVec3::new(1.0, 0.0, 0.0),
            Self::Custom(v) => v.normalize(),
        }
    }
}

/// Transforms a local bounding box into parent/world space by applying the spatial transform.
///
/// Under the hood, this extracts the 4x4 double-precision transformation matrix,
/// transforms all corners of the bounding box, and computes the tightest axis-aligned bounding box.
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

/// Computes the spatial translation shift required to align an anchor point on the target object
/// with an anchor point on the reference object.
pub fn compute_align_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    target_anchor: LayoutAnchor,
    ref_anchor: LayoutAnchor,
) -> DVec3 {
    let target_world_bounds = transform_bounds(target_bounds, target_transform);
    let ref_world_bounds = transform_bounds(ref_bounds, ref_transform);

    let p_ref = ref_anchor.get_point(&ref_world_bounds);
    let p_target = target_anchor.get_point(&target_world_bounds);

    p_ref - p_target
}

/// Computes the spatial translation shift required to place the target object adjacent to
/// the reference object in a specific direction with given spacing.
///
/// Orthogonal axes are automatically centered relative to the reference object (like Manim).
pub fn compute_next_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    direction: LayoutDirection,
    spacing: f64,
) -> DVec3 {
    let target_world_bounds = transform_bounds(target_bounds, target_transform);
    let ref_world_bounds = transform_bounds(ref_bounds, ref_transform);

    match direction {
        LayoutDirection::Right => DVec3::new(
            ref_world_bounds.max.x + spacing - target_world_bounds.min.x,
            ref_world_bounds.center().y - target_world_bounds.center().y,
            ref_world_bounds.center().z - target_world_bounds.center().z,
        ),
        LayoutDirection::Left => DVec3::new(
            ref_world_bounds.min.x - spacing - target_world_bounds.max.x,
            ref_world_bounds.center().y - target_world_bounds.center().y,
            ref_world_bounds.center().z - target_world_bounds.center().z,
        ),
        LayoutDirection::Up => DVec3::new(
            ref_world_bounds.center().x - target_world_bounds.center().x,
            ref_world_bounds.max.y + spacing - target_world_bounds.min.y,
            ref_world_bounds.center().z - target_world_bounds.center().z,
        ),
        LayoutDirection::Down => DVec3::new(
            ref_world_bounds.center().x - target_world_bounds.center().x,
            ref_world_bounds.min.y - spacing - target_world_bounds.max.y,
            ref_world_bounds.center().z - target_world_bounds.center().z,
        ),
        LayoutDirection::Custom(v) => {
            // Fallback for custom vector directions: project target center alongside standard spacing
            let dir = v.normalize();
            let p_ref = ref_world_bounds.center() + dir * (ref_world_bounds.size().dot(dir) * 0.5 + spacing);
            let p_target = target_world_bounds.center() - dir * (target_world_bounds.size().dot(dir) * 0.5);
            p_ref - p_target
        }
    }
}
