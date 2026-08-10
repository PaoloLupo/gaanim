use gaanim_core::glam::{DVec2, DVec3};
use gaanim_math::Bounds3D;

/// 9-point anchor system inspired by Manim's critical points
/// and Motion Canvas's offset concept.
///
/// Each anchor maps to a normalized [-1..1, -1..1] offset within
/// a bounding box. This allows computing the world-space position
/// of any reference point on an object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Anchor {
    #[default]
    Center,
    Top,         // (0, 1)   — edge center top
    Bottom,      // (0, -1)  — edge center bottom
    Left,        // (-1, 0)  — edge center left
    Right,       // (1, 0)   — edge center right
    TopLeft,     // (-1, 1)
    TopRight,    // (1, 1)
    BottomLeft,  // (-1, -1)
    BottomRight, // (1, -1)
}

impl Anchor {
    /// Returns the normalized offset `[-1..1]` for the anchor.
    pub fn to_offset(&self) -> DVec2 {
        match self {
            Self::Center => DVec2::new(0.0, 0.0),
            Self::Top => DVec2::new(0.0, 1.0),
            Self::Bottom => DVec2::new(0.0, -1.0),
            Self::Left => DVec2::new(-1.0, 0.0),
            Self::Right => DVec2::new(1.0, 0.0),
            Self::TopLeft => DVec2::new(-1.0, 1.0),
            Self::TopRight => DVec2::new(1.0, 1.0),
            Self::BottomLeft => DVec2::new(-1.0, -1.0),
            Self::BottomRight => DVec2::new(1.0, -1.0),
        }
    }

    /// Computes the point location of this anchor on a bounding box.
    pub fn get_point(&self, bounds: &Bounds3D) -> DVec3 {
        let center = bounds.center();
        let half_size = bounds.size() * 0.5;
        let offset2 = self.to_offset();
        DVec3::new(
            center.x + offset2.x * half_size.x,
            center.y + offset2.y * half_size.y,
            center.z,
        )
    }

    /// Derives the nearest discrete anchor from a direction vector.
    pub fn from_direction(dir: DVec3) -> Self {
        if dir.length_squared() < 1e-6 {
            return Self::Center;
        }
        let dir = dir.normalize();
        let th = 0.38268; // sin(22.5 degrees)
        let x = dir.x;
        let y = dir.y;

        let dx = if x > th {
            1
        } else if x < -th {
            -1
        } else {
            0
        };
        let dy = if y > th {
            1
        } else if y < -th {
            -1
        } else {
            0
        };

        match (dx, dy) {
            (0, 1) => Self::Top,
            (0, -1) => Self::Bottom,
            (-1, 0) => Self::Left,
            (1, 0) => Self::Right,
            (-1, 1) => Self::TopLeft,
            (1, 1) => Self::TopRight,
            (-1, -1) => Self::BottomLeft,
            (1, -1) => Self::BottomRight,
            _ => Self::Center,
        }
    }
}
