pub mod ops;
mod segment;
pub use crate::anim::BoundsTarget;
pub use gaanim_animation::AxisMask;
pub use gaanim_animation::{RollingMode, RollingNumberOptions};
pub use gaanim_renderer::background::{BackgroundPaint, ShaderBackground, ShaderBackgroundError};
pub use gaanim_renderer::background_presets;
pub use gaanim_renderer::effects::{
    CameraViewBackground, CameraViewFit, ConnectMode, DropShadow, GaussianBlur, Glow, MotionBlur,
    StrokeAlign, StrokeProfile,
};
pub use gaanim_renderer::post_presets::{CubeLut, PostPreset};
pub use gaanim_renderer::post_process::{
    PostProcessError, PostProcessOverride, PostProcessPass, PostProcessShader,
};
pub use gaanim_renderer::transition_presets::{
    LUMA_MAP_MAX, LumaMap, TransitionPreset, luma_map, luma_map_from_file,
};
pub use ops::{
    AnchorPoint, CanvasEndpoint, CanvasRay, FragmentRevealStyle, PointRef, UpdaterPreset,
};
mod types;
pub use gaanim_layout::{Anchor, Direction};
pub use gaanim_text::prelude::TextAnchor;
pub use segment::SceneMarker;
pub use segment::{
    PresentationBrand, SegmentError, SegmentHandle, SegmentId, SegmentManifest, SegmentSpec,
    SegmentStop,
};
pub use types::{
    Anim, Axes3DConfig, AxesConfig, BooleanOperation, BooleanRule, CurveControl, CurveElement,
    EchoSpec, FillLevelDirection, ImageCrop, ImageFit, ImageOptions, ImageOptionsError, LabelMode,
    LayoutMemberSpec, LayoutOp, LayoutSpec, LayoutTreeSnapshot, LayoutWithin, LinearMap2D,
    LinearMapError, LottieOptions, MAX_ECHO_COUNT, Margin, ObjectSpec, OptDuration, SceneFrame,
    SpawnKind, VideoOptions,
};
/// Raster image handle; remains compatible with every DrawableHandle consumer.
pub type ImageHandle = DrawableHandle;

mod audio_signals;
pub use gaanim_media::analysis::AnalysisError as AudioAnalysisError;
mod bar_race;
pub use bar_race::{
    BAR_RACE_PALETTE, BarRace, BarRaceBar, BarRaceLabels, BarRaceOptions, MAX_AUTO_FONT_FRACTION,
};
pub use gaanim_visualization::{BarRaceModel, ValueFormat};
mod camera_view;
mod drawable;
mod duplicate;
mod emphasis;
pub use duplicate::{Distribution, MAX_COPIES, RepeatStep};
mod particles;
mod path_modifiers;
pub use path_modifiers::{
    MAX_DETAIL, MAX_OFFSET_COPIES, MAX_RIDGES, OffsetJoin, PathModifierHandle,
};
mod property_bindings;
pub use camera_view::{
    CameraInsetOptions, CameraInsetPlacement, CameraInsetShape, CameraViewError, CameraViewHandle,
    CameraViewOptions, CameraViewZoom,
};
pub use drawable::{
    ClipOptions, DrawableHandle, FragmentSelection, ImagePixelError, LayoutOwnershipError,
    Primitive3DHandleError, RotationAxisError, SvgPartError,
};
pub use particles::{
    Emitter, EmitterShape, GRADIENT_PARTICLE_COLORS, MAX_BURST, MAX_LIVE_PARTICLES,
    MAX_PARTICLE_COLORS, ParticleColors, ParticleOptions, ParticleShape, ParticleSpawn,
    confetti_palette,
};
mod editorial;
pub use editorial::{
    BadgeSpec, BannerPosition, BannerSpec, CardSpec, ChipSpec, EditorialAlign, EditorialAppearance,
    EditorialError, EditorialStyle, EditorialVariant, LowerThirdSide, LowerThirdSpec,
    QuoteCardSpec, SectionHeaderSpec, StatCardSpec,
};
mod magic_move;
pub use magic_move::{
    KeyedPairs, MagicMoveError, MagicMoveFailure, MagicMoveKey, MagicMoveUnmatched, pair_by_key,
};
mod view_ticks;
mod visualization;
pub use gaanim_visualization::{
    Cartesian3DVisibility, CartesianVisibility, NumberLineVisibility, PolarVisibility,
};
pub use visualization::{
    ArrowFieldOptions, ArrowVectorFieldHandle, ChartHandle, CoordinateRef, CoordinateSpace3DHandle,
    CoordinateSpaceHandle, FlowParticleOptions, FlowParticlesHandle, NumberLineHandle, Parameter,
    PolarSpaceHandle, ProgressLabel, ProgressRing, ProgressRingOptions, StreamLinesHandle,
    StreamLinesStyle, VectorField2DHandle, VectorField3DHandle, VisualizationError,
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
    StopLoopError, SupportHandle, SurroundingRectError, SurroundingRectHandle, ThemeError,
    TypstAssetError, VideoClip, VideoLoadError, VideoSegment, stagger_weights,
};
pub use canvas_impl::{InsertPosition, ScheduleLabel};
pub use gaanim_media::narration::{MarkerSource, ScriptSection, TakeFiles};
pub use narration::{
    LiveTakeSpec, MarkerSpec, NarrationManifest, ScriptSpec, TextSource, VoiceoverError,
    VoiceoverHandle, VoiceoverSpec, live_take_recording, set_live_take_recording,
};
mod character;
pub use character::{CharacterError, CharacterHandle};
mod live;
pub use gaanim_animation::live::{
    LiveZone, Motion as LiveMotion, Program as LiveProgram, ProgramError as LiveProgramError,
    ZoneNames as LiveZoneNames, ZoneState as LiveZoneState,
};
pub use gaanim_objects::character::Character;
pub use live::{LiveZoneError, LiveZoneHandle, check_behavior};
mod poll;
pub use gaanim_animation::polls::{BarDirection, BarScale, TextAlign};
pub use poll::{
    ANSWER_COLORS, AudienceHandle, GateCondition, LeaderboardHandle, LiveTextOptions,
    MAX_POLL_OPTIONS, MAX_TEAMS, POLL_IMAGE_SIZE, PlayerFact, PollBarOptions, PollError,
    PollHandle, PollImage, PollSession, PollStyle, QUIZ_POINTS, QUIZ_TIME, REVEAL_AFTER,
    TEAM_COLORS, TeamsHandle, poll_image,
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
mod measure;
pub use measure::{BoundsError, ScatterLayoutError, avoid_boxes, scatter};
mod text_motion;
pub(crate) use incremental::SceneFingerprints;
pub mod text_animator;
pub use text_animator::{
    SelectorShape, TextAnimator, TextAnimatorError, TextAnimatorOut, TextRevealStyle,
};

/// dotLottie selectors and typed inputs.
pub use gaanim_renderer::lottie::{LottieInput, LottiePackageOptions};
