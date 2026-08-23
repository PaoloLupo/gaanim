pub use glam;
pub use kurbo;
pub use kurbo::{Affine, BezPath, Point, Rect, Shape, Size, Vec2};
pub use peniko;
pub use peniko::{Brush, Color, Fill};
pub use thiserror::Error;

pub mod color;
pub mod colormap;
pub use color::{interpolate_color, interpolate_rgba8};
pub use colormap::{ColorMap, ColorMapError};

pub mod id;
pub use id::ObjectId;

pub mod theme;
pub use theme::Theme;

#[derive(Error, Debug)]
pub enum GaanimError {
    #[error("entity not found: {0}")]
    EntityNotFound(ObjectId),
}

pub type Result<T> = std::result::Result<T, GaanimError>;
