use gaanim_api::canvas::{Anchor, Direction};
use gaanim_core::glam::DVec3;
use pyo3::prelude::*;

#[pyclass(name = "Anchor", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyAnchor(pub Anchor);

#[pymethods]
#[allow(non_snake_case)]
impl PyAnchor {
    #[classattr]
    fn CENTER() -> Self {
        Self(Anchor::Center)
    }

    #[classattr]
    fn TOP() -> Self {
        Self(Anchor::Top)
    }

    #[classattr]
    fn BOTTOM() -> Self {
        Self(Anchor::Bottom)
    }

    #[classattr]
    fn LEFT() -> Self {
        Self(Anchor::Left)
    }

    #[classattr]
    fn RIGHT() -> Self {
        Self(Anchor::Right)
    }

    #[classattr]
    fn TOP_LEFT() -> Self {
        Self(Anchor::TopLeft)
    }

    #[classattr]
    fn TOP_RIGHT() -> Self {
        Self(Anchor::TopRight)
    }

    #[classattr]
    fn BOTTOM_LEFT() -> Self {
        Self(Anchor::BottomLeft)
    }

    #[classattr]
    fn BOTTOM_RIGHT() -> Self {
        Self(Anchor::BottomRight)
    }

    fn __repr__(&self) -> &'static str {
        match self.0 {
            Anchor::Center => "Anchor.CENTER",
            Anchor::Top => "Anchor.TOP",
            Anchor::Bottom => "Anchor.BOTTOM",
            Anchor::Left => "Anchor.LEFT",
            Anchor::Right => "Anchor.RIGHT",
            Anchor::TopLeft => "Anchor.TOP_LEFT",
            Anchor::TopRight => "Anchor.TOP_RIGHT",
            Anchor::BottomLeft => "Anchor.BOTTOM_LEFT",
            Anchor::BottomRight => "Anchor.BOTTOM_RIGHT",
        }
    }
}

#[pyclass(name = "Direction", module = "gaanim_core", frozen, from_py_object)]
#[derive(Clone, Copy, Debug)]
pub struct PyDirection(pub Direction);

#[pymethods]
#[allow(non_snake_case)]
impl PyDirection {
    #[classattr]
    fn UP() -> Self {
        Self(Direction::Up)
    }

    #[classattr]
    fn DOWN() -> Self {
        Self(Direction::Down)
    }

    #[classattr]
    fn LEFT() -> Self {
        Self(Direction::Left)
    }

    #[classattr]
    fn RIGHT() -> Self {
        Self(Direction::Right)
    }

    #[classattr]
    fn UP_LEFT() -> Self {
        Self(Direction::UpLeft)
    }

    #[classattr]
    fn UP_RIGHT() -> Self {
        Self(Direction::UpRight)
    }

    #[classattr]
    fn DOWN_LEFT() -> Self {
        Self(Direction::DownLeft)
    }

    #[classattr]
    fn DOWN_RIGHT() -> Self {
        Self(Direction::DownRight)
    }

    #[staticmethod]
    #[pyo3(signature = (x, y, z = 0.0))]
    fn custom(x: f64, y: f64, z: f64) -> Self {
        Self(Direction::Custom(DVec3::new(x, y, z)))
    }

    fn __repr__(&self) -> String {
        match self.0 {
            Direction::Up => "Direction.UP".to_string(),
            Direction::Down => "Direction.DOWN".to_string(),
            Direction::Left => "Direction.LEFT".to_string(),
            Direction::Right => "Direction.RIGHT".to_string(),
            Direction::UpLeft => "Direction.UP_LEFT".to_string(),
            Direction::UpRight => "Direction.UP_RIGHT".to_string(),
            Direction::DownLeft => "Direction.DOWN_LEFT".to_string(),
            Direction::DownRight => "Direction.DOWN_RIGHT".to_string(),
            Direction::Custom(v) => {
                format!("Direction.custom({}, {}, {})", v.x, v.y, v.z)
            }
        }
    }
}
