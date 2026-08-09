pub mod ops;
mod segment;
pub use gaanim_animation::AxisMask;
pub use ops::{CanvasEndpoint, FragmentRevealStyle, UpdaterPreset};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use types::{
    Anim, Axes3DConfig, AxesConfig, CanvasUnits, CurveControl, CurveElement, ImageCrop, ImageFit,
    ImageOptions, ImageOptionsError, LabelMode, LayoutKind, LayoutOp, Margin, ObjectSpec,
    OptDuration, ParagraphOptions, ParagraphOverflow, SpawnKind, TextAlign,
};
mod layout;
pub use layout::{FrameLayout, GridLayout, GridTrack, LayoutPreset, LayoutRegion};
pub use segment::{
    PresentationBrand, SegmentError, SegmentHandle, SegmentId, SegmentLayout, SegmentManifest,
    SegmentSpec, SegmentStop,
};
mod drawable;
pub use drawable::{
    DrawableHandle, FragmentSelection, GltfAnimationError, RotationAxisError, SvgPartError,
};
mod visualization;
pub use visualization::{
    CoordinateRef, CoordinateSpace3DHandle, CoordinateSpaceHandle, NumberLineHandle, Parameter,
    PolarSpaceHandle, VisualizationError,
};
mod canvas_impl;
pub use crate::export::{AudioTrack, AudioTrackError};
pub use canvas_impl::{
    AssetPreloadError, AssetRootError, Canvas, ImageLoadError, SceneObjectError, ThemeError,
};
mod theme;
pub use gaanim_objects::prelude::SvgLoadError;
pub use theme::{CanvasTheme, ThemeFont, ThemePalette};
mod compile;
pub(crate) use compile::{split_text_math, text_inline_typst_source};
