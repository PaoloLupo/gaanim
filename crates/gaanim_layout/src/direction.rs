use crate::Anchor;
use gaanim_core::glam::DVec3;
use std::ops::Add;

/// Combinable direction type. Supports UP + LEFT = TopLeft style composition.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
    // Diagonal combinations
    UpLeft,
    UpRight,
    DownLeft,
    DownRight,
    // Arbitrary vector
    Custom(DVec3),
}

impl Direction {
    /// Returns the unit direction vector.
    pub fn to_vector(&self) -> DVec3 {
        match self {
            Self::Up => DVec3::new(0.0, 1.0, 0.0),
            Self::Down => DVec3::new(0.0, -1.0, 0.0),
            Self::Left => DVec3::new(-1.0, 0.0, 0.0),
            Self::Right => DVec3::new(1.0, 0.0, 0.0),
            Self::UpLeft => DVec3::new(-1.0, 1.0, 0.0).normalize(),
            Self::UpRight => DVec3::new(1.0, 1.0, 0.0).normalize(),
            Self::DownLeft => DVec3::new(-1.0, -1.0, 0.0).normalize(),
            Self::DownRight => DVec3::new(1.0, -1.0, 0.0).normalize(),
            Self::Custom(v) => {
                if v.length_squared() > 1e-6 {
                    v.normalize()
                } else {
                    DVec3::ZERO
                }
            }
        }
    }

    /// Returns the opposite direction.
    pub fn opposite(&self) -> Self {
        match self {
            Self::Up => Self::Down,
            Self::Down => Self::Up,
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::UpLeft => Self::DownRight,
            Self::UpRight => Self::DownLeft,
            Self::DownLeft => Self::UpRight,
            Self::DownRight => Self::UpLeft,
            Self::Custom(v) => Self::Custom(-*v),
        }
    }

    /// Maps the direction to its natural Anchor.
    pub fn to_anchor(&self) -> Anchor {
        match self {
            Self::Up => Anchor::Top,
            Self::Down => Anchor::Bottom,
            Self::Left => Anchor::Left,
            Self::Right => Anchor::Right,
            Self::UpLeft => Anchor::TopLeft,
            Self::UpRight => Anchor::TopRight,
            Self::DownLeft => Anchor::BottomLeft,
            Self::DownRight => Anchor::BottomRight,
            Self::Custom(v) => Anchor::from_direction(*v),
        }
    }
}

impl Add<Direction> for Direction {
    type Output = Direction;

    fn add(self, rhs: Direction) -> Self::Output {
        let v1 = self.to_vector();
        let v2 = rhs.to_vector();
        let sum = v1 + v2;
        if sum.length_squared() < 1e-6 {
            Direction::Custom(DVec3::ZERO)
        } else {
            let th = 0.1;
            let sx = sum.x.signum();
            let sy = sum.y.signum();
            let x_nonzero = sum.x.abs() > th;
            let y_nonzero = sum.y.abs() > th;

            match (x_nonzero, y_nonzero) {
                (true, true) => {
                    if sx > 0.0 && sy > 0.0 {
                        Direction::UpRight
                    } else if sx < 0.0 && sy > 0.0 {
                        Direction::UpLeft
                    } else if sx > 0.0 && sy < 0.0 {
                        Direction::DownRight
                    } else {
                        Direction::DownLeft
                    }
                }
                (true, false) => {
                    if sx > 0.0 {
                        Direction::Right
                    } else {
                        Direction::Left
                    }
                }
                (false, true) => {
                    if sy > 0.0 {
                        Direction::Up
                    } else {
                        Direction::Down
                    }
                }
                (false, false) => Direction::Custom(DVec3::ZERO),
            }
        }
    }
}
