pub mod ops;
mod segment;
pub use gaanim_animation::AxisMask;
pub use ops::{CanvasEndpoint, FragmentRevealStyle, UpdaterPreset};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use segment::{
    PresentationBrand, SegmentError, SegmentHandle, SegmentId, SegmentManifest, SegmentSpec,
    SegmentStop,
};
pub use types::{
    Anim, Axes3DConfig, AxesConfig, CanvasUnits, CurveControl, CurveElement, ImageCrop, ImageFit,
    ImageOptions, ImageOptionsError, LabelMode, LayoutMemberSpec, LayoutOp, LayoutSpec,
    LayoutTreeSnapshot, LayoutWithin, Margin, ObjectSpec, OptDuration, SpawnKind,
};
mod drawable;
pub use drawable::{
    DrawableHandle, FragmentSelection, GltfAnimationError, LayoutOwnershipError,
    Primitive3DHandleError, RotationAxisError, SvgPartError,
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
pub use theme::{
    CanvasTheme, LayoutTokens, ThemeFont, ThemePaint, ThemePalette, ThemeStrokeStyle, ThemeStyle,
};
mod compile;
pub(crate) use compile::{split_text_math, text_inline_typst_source};
