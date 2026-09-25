pub mod ops;
mod segment;
pub use crate::anim::BoundsTarget;
pub use gaanim_animation::AxisMask;
pub use gaanim_animation::{RollingMode, RollingNumberOptions};
pub use gaanim_renderer::background::{BackgroundPaint, ShaderBackground, ShaderBackgroundError};
pub use gaanim_renderer::effects::{DropShadow, GaussianBlur, Glow};
pub use gaanim_renderer::post_process::{PostProcessError, PostProcessOverride, PostProcessShader};
pub use ops::{
    AnchorPoint, CanvasEndpoint, CanvasRay, FragmentRevealStyle, PointRef, UpdaterPreset,
};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use gaanim_text::prelude::TextAnchor;
pub use segment::{
    PresentationBrand, SegmentError, SegmentHandle, SegmentId, SegmentManifest, SegmentSpec,
    SegmentStop,
};
pub use types::{
    Anim, Axes3DConfig, AxesConfig, BooleanOperation, BooleanRule, CurveControl, CurveElement,
    FillLevelDirection, ImageCrop, ImageFit, ImageOptions, ImageOptionsError, LabelMode,
    LayoutMemberSpec, LayoutOp, LayoutSpec, LayoutTreeSnapshot, LayoutWithin, LottieOptions,
    Margin, ObjectSpec, OptDuration, SceneFrame, SpawnKind, VideoOptions,
};
/// Raster image handle; remains compatible with every DrawableHandle consumer.
pub type ImageHandle = DrawableHandle;

mod drawable;
mod property_bindings;
pub use drawable::{
    ClipOptions, DrawableHandle, FragmentSelection, GltfAnimationError, LayoutOwnershipError,
    Primitive3DHandleError, RotationAxisError, SvgPartError,
};
mod editorial;
pub use editorial::{
    BadgeSpec, BannerPosition, BannerSpec, CardSpec, ChipSpec, EditorialAlign, EditorialAppearance,
    EditorialError, EditorialStyle, EditorialVariant, LowerThirdSide, LowerThirdSpec,
    QuoteCardSpec, SectionHeaderSpec, StatCardSpec,
};
mod view_ticks;
mod visualization;
pub use gaanim_visualization::{
    Cartesian3DVisibility, CartesianVisibility, NumberLineVisibility, PolarVisibility,
};
pub use visualization::{
    ArrowFieldOptions, ArrowVectorFieldHandle, ChartHandle, CoordinateRef, CoordinateSpace3DHandle,
    CoordinateSpaceHandle, FlowParticleOptions, FlowParticlesHandle, NumberLineHandle, Parameter,
    PolarSpaceHandle, StreamLinesHandle, StreamLinesStyle, VectorField2DHandle,
    VectorField3DHandle, VisualizationError,
};
mod canvas_impl;
mod narration;
pub use crate::export::{AudioTrack, AudioTrackError};
pub use canvas_impl::clear_asset_caches;
pub use canvas_impl::{
    AngleDimensionHandle, AngleDimensionOptions, AssetPreloadError, AssetRootError, AudioClip,
    BooleanError, CameraBindingError, CameraConstraintHandle, CameraStateError, CameraStateHandle,
    Composition, DEFAULT_REACTIVE_TEXT_SIZE, DimensionExtensionStyle, DimensionHandle,
    DimensionOptions, ForceVectorHandle, ImageLoadError, LottieClip, LottieLoadError, PlayError,
    PlayItem, SceneModel, SceneObjectError, Schedule, ScheduleEntry, StaggerLayout, StaggerOrigin,
    SupportHandle, SurroundingRectError, SurroundingRectHandle, ThemeError, TypstAssetError,
    VideoClip, VideoLoadError, VideoSegment, stagger_weights,
};
pub use gaanim_media::narration::{MarkerSource, ScriptSection, TakeFiles};
pub use narration::{
    LiveTakeSpec, MarkerSpec, NarrationManifest, ScriptSpec, TextSource, VoiceoverError,
    VoiceoverHandle, VoiceoverSpec, live_take_recording, set_live_take_recording,
};
mod theme;
pub use gaanim_objects::prelude::SvgLoadError;
pub use theme::{
    CanvasTheme, LayoutTokens, ThemeFont, ThemePaint, ThemePalette, ThemeStrokeStyle, ThemeStyle,
};
mod compile;
pub(crate) use compile::{
    CompileCheckpoint, SegmentMarker, split_text_math, text_inline_typst_source,
};
mod incremental;
pub(crate) use incremental::SceneFingerprints;
pub mod text_animator;
pub use text_animator::{
    SelectorShape, TextAnimator, TextAnimatorError, TextAnimatorOut, TextRevealStyle,
};

/// dotLottie selectors and typed inputs.
pub use gaanim_renderer::lottie::{LottieInput, LottiePackageOptions};
