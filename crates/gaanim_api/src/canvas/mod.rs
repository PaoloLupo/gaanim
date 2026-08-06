mod ops;
mod presentation;
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
pub use presentation::{
    PresentationBrand, PresentationError, PresentationManifest, SlideId, SlideSpec, SlideStep,
    SlideTemplate,
};
mod drawable;
pub use drawable::{DrawableHandle, FragmentSelection, SvgPartError};
mod canvas_impl;
pub use crate::export::{AudioTrack, AudioTrackError};
pub use canvas_impl::{AssetPreloadError, AssetRootError, Canvas, ImageLoadError, ThemeError};
mod theme;
pub use gaanim_objects::prelude::SvgLoadError;
pub use theme::{CanvasTheme, ThemeFont, ThemePalette};
mod compile;
pub(crate) use compile::{split_text_math, text_inline_typst_source};
