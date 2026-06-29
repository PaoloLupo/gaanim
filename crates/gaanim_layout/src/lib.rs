use gaanim_core::glam::DVec3;
use gaanim_math::{Bounds3D, SpatialTransform};

pub mod anchor;
pub mod arrange;
pub mod direction;
pub mod positioning;
pub mod query;

pub use anchor::Anchor;
pub use arrange::{arrange, arrange_in_grid, hstack, vstack};
pub use direction::Direction;
pub use positioning::{
    compute_align_to as compute_align_to_new, compute_move_to,
    compute_next_to as compute_next_to_new, compute_to_corner, compute_to_edge, transform_bounds,
};
pub use query::{get_anchor_point, get_center, get_corner, get_edge_center, get_height, get_width};

/// Legacy layout anchors representing discrete alignment points on a bounding box.
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

/// Legacy layout directions.
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

/// Legacy compute_align_to.
pub fn compute_align_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    target_anchor: LayoutAnchor,
    ref_anchor: LayoutAnchor,
) -> DVec3 {
    positioning::compute_align_to(
        target_bounds,
        target_transform,
        ref_bounds,
        ref_transform,
        Anchor::from(target_anchor),
        Anchor::from(ref_anchor),
    )
}

/// Legacy compute_next_to.
pub fn compute_next_to(
    target_bounds: Bounds3D,
    target_transform: &SpatialTransform,
    ref_bounds: Bounds3D,
    ref_transform: &SpatialTransform,
    direction: LayoutDirection,
    spacing: f64,
) -> DVec3 {
    positioning::compute_next_to(
        target_bounds,
        target_transform,
        ref_bounds,
        ref_transform,
        Direction::from(direction),
        spacing,
        Anchor::Center,
    )
}
