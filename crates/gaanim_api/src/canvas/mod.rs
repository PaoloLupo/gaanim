mod ops;
pub use gaanim_animation::AxisMask;
pub use ops::{CanvasEndpoint, UpdaterPreset};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use types::{
    Anim, AxesConfig, CoordinateSystem, ImageCrop, ImageFit, ImageOptions, ImageOptionsError,
    LayoutOp, Margin, ObjectSpec, OptDuration, SpawnKind,
};
mod drawable;
pub use drawable::DrawableHandle;
mod canvas_impl;
pub use canvas_impl::{Canvas, ImageLoadError};
pub use gaanim_objects::prelude::SvgLoadError;
mod compile;
