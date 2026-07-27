mod ops;
pub use gaanim_animation::AxisMask;
pub use ops::{CanvasEndpoint, FragmentRevealStyle, UpdaterPreset};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use types::{
    Anim, AxesConfig, CoordinateSystem, CurveControl, CurveElement, ImageCrop, ImageFit,
    ImageOptions, ImageOptionsError, LayoutKind, LayoutOp, Margin, ObjectSpec, OptDuration,
    ParagraphOptions, ParagraphOverflow, SpawnKind, TextAlign,
};
mod layout;
pub use layout::{FrameLayout, GridLayout, GridTrack, LayoutPreset, LayoutRegion};
mod drawable;
pub use drawable::{DrawableHandle, FragmentSelection};
mod canvas_impl;
pub use crate::export::{AudioTrack, AudioTrackError};
pub use canvas_impl::{AssetPreloadError, AssetRootError, Canvas, ImageLoadError, ThemeError};
pub use gaanim_objects::prelude::SvgLoadError;
mod compile;
