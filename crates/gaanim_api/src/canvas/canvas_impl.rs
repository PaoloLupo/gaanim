//! SceneModel — the top-level facade for building Gaanim animations.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use bevy::prelude::*;
use gaanim_animation::ScalarSource;
use gaanim_core::glam::{DVec2, DVec3};
use gaanim_core::kurbo::{Cap, Shape, Stroke};
use gaanim_core::peniko::{Brush, Color};
use gaanim_math::RateFunc;
use gaanim_objects::prelude::SvgLoadError;
use gaanim_objects::primitives::{
    DEFAULT_ARROW_BODY_WIDTH, DEFAULT_ARROW_HEAD_LENGTH, DEFAULT_ARROW_HEAD_WIDTH,
};
use gaanim_objects::primitives3d;
use gaanim_text::prelude::TextRole;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType, BoundsTarget};
use crate::canvas::SceneMarker;
use crate::canvas::authored::Authored;
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{
    CameraBindingSpec, CanvasCameraBindingKind, CanvasEndpoint, CanvasRay, CanvasState,
    LocalSegmentStop, Op, PointRef, Segment, SharedCameraBindingSpec, SharedCanvasState,
};
use crate::canvas::types::{
    Anim, BooleanOperation, BooleanRule, FillLevelDirection, ImageFit, ImageOptions,
    ImageOptionsError, LayoutMemberSpec, LayoutSpec, LayoutTreeSnapshot, LottieOptions, Margin,
    ReactiveReadoutLayoutSpec, SceneFrame, SpawnKind, VideoOptions,
};
use crate::canvas::{
    Anchor, CanvasTheme, PresentationBrand, SegmentError, SegmentHandle, SegmentManifest,
    SegmentSpec, SegmentStop,
};
use crate::export::{AudioTrack, AudioTrackError};

/// A validated audio declaration that can be activated by [`SceneModel::play_items`].
#[derive(Debug, Clone)]
pub struct AudioClip {
    pub(crate) track: AudioTrack,
    state: SharedCanvasState,
    /// Whether the track's duration lengthens the play that activates it.
    extends_play: bool,
    /// The tracks this clip has played, with their start times; audio
    /// signals read them to place the analysis on the timeline.
    pub(crate) plays: Arc<Mutex<Vec<AudioTrack>>>,
}

/// A timeline-synchronized video declaration activated by [`SceneModel::play_items`].
#[derive(Debug, Clone)]
pub struct VideoClip {
    pub drawable: DrawableHandle,
    state: SharedCanvasState,
    duration: Option<f64>,
    audio: Option<AudioTrack>,
    activated: Arc<AtomicBool>,
}

/// A finite selection of a video, consumed once by Scene.play.
#[derive(Debug, Clone)]
pub struct VideoSegment {
    video: VideoClip,
    interval: gaanim_media::VideoInterval,
    audio: Option<AudioTrack>,
    consumed: Arc<AtomicBool>,
}

/// A timeline-synchronized Lottie declaration activated by [`SceneModel::play_items`].
#[derive(Debug, Clone)]
pub struct LottieClip {
    pub drawable: DrawableHandle,
    state: SharedCanvasState,
    duration: Option<f64>,
    activated: Arc<AtomicBool>,
    asset: Arc<gaanim_renderer::lottie::LottieAsset>,
}

impl VideoClip {
    pub fn frame(mut self, width: f64, height: f64, fit: ImageFit) -> Result<Self, &'static str> {
        self.drawable = self.drawable.frame(width, height, fit)?;
        Ok(self)
    }
    pub fn crop(
        mut self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        normalized: bool,
    ) -> Result<Self, &'static str> {
        self.drawable = self.drawable.crop(x, y, width, height, normalized)?;
        Ok(self)
    }
    pub fn quality(
        mut self,
        quality: gaanim_core::peniko::ImageQuality,
    ) -> Result<Self, &'static str> {
        self.drawable = self.drawable.quality(quality)?;
        Ok(self)
    }
    pub fn animate(&self) -> Anim {
        self.drawable.animate()
    }
    pub fn move_to(mut self, x: f64, y: f64) -> Self {
        self.drawable = self.drawable.move_to(x, y);
        self
    }
    pub fn scale_to(mut self, factor: f64) -> Self {
        self.drawable = self.drawable.scale_to(factor);
        self
    }
    pub fn opacity(mut self, opacity: f32) -> Self {
        self.drawable = self.drawable.opacity(opacity);
        self
    }

    pub fn source_width(&self) -> u32 {
        self.metadata().width
    }
    pub fn source_height(&self) -> u32 {
        self.metadata().height
    }
    pub fn source_duration(&self) -> f64 {
        self.metadata().duration
    }
    pub fn frame_rate(&self) -> f64 {
        self.metadata().fps
    }
    fn metadata(&self) -> gaanim_media::VideoMetadata {
        let spec = self.drawable.spec.lock().expect("object spec poisoned");
        let SpawnKind::Video { playback, .. } = &spec.kind else {
            unreachable!()
        };
        playback.metadata.clone()
    }
    /// Select absolute source seconds; omitted controls inherit the declaration.
    pub fn segment(
        &self,
        start: f64,
        end: f64,
        speed: Option<f64>,
        audio: Option<bool>,
        volume: Option<f64>,
    ) -> Result<VideoSegment, VideoLoadError> {
        let spec = self.drawable.spec.lock().expect("object spec poisoned");
        let SpawnKind::Video { playback, .. } = &spec.kind else {
            unreachable!()
        };
        let speed = speed.unwrap_or(playback.speed);
        let volume = volume.unwrap_or(playback.volume);
        if !start.is_finite()
            || !end.is_finite()
            || start < 0.0
            || end <= start
            || end > playback.metadata.duration
        {
            return Err(VideoLoadError::DurationOutOfRange);
        }
        for (name, value, positive) in [("speed", speed, true), ("volume", volume, false)] {
            if !value.is_finite() || if positive { value <= 0.0 } else { value < 0.0 } {
                return Err(VideoLoadError::InvalidNumber {
                    name,
                    requirement: if positive { "positive" } else { "non-negative" },
                });
            }
        }
        let track = if audio.unwrap_or(playback.audio) && playback.metadata.has_audio {
            Some(AudioTrack::from_media(
                playback.path.clone(),
                0.0,
                start,
                end - start,
                speed,
                false,
                volume,
            )?)
        } else {
            None
        };
        Ok(VideoSegment {
            video: self.clone(),
            interval: gaanim_media::VideoInterval {
                scene_start: 0.0,
                source_start: start,
                source_end: end,
                speed,
            },
            audio: track,
            consumed: Arc::new(AtomicBool::new(false)),
        })
    }
    fn belongs_to(&self, state: &SharedCanvasState) -> bool {
        Arc::ptr_eq(&self.state, state)
    }
}

impl LottieClip {
    fn command(
        &self,
        command: gaanim_renderer::lottie::LottieCommand,
    ) -> Result<(), LottieLoadError> {
        let time: f64 = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .map(|s| s.cursor)
            .sum();
        let mut spec = self.drawable.spec.lock().expect("object spec poisoned");
        let SpawnKind::Lottie { playback } = &mut spec.kind else {
            unreachable!()
        };
        let package = playback.package.as_mut().ok_or_else(|| {
            gaanim_renderer::lottie::LottieError::Package(
                "package controls require a .lottie file".into(),
            )
        })?;
        package.record(command, time, playback.scene_start, playback.active)?;
        Ok(())
    }
    /// Apply a static package theme at the cursor, or restore the base with None.
    pub fn set_theme(self, id: Option<&str>) -> Result<Self, LottieLoadError> {
        self.command(gaanim_renderer::lottie::LottieCommand::Theme(
            id.map(str::to_owned),
        ))?;
        Ok(self)
    }
    /// Set a typed machine input without advancing the cursor.
    pub fn set_input(
        self,
        name: &str,
        value: gaanim_renderer::lottie::LottieInput,
    ) -> Result<Self, LottieLoadError> {
        self.command(gaanim_renderer::lottie::LottieCommand::Input(
            name.to_owned(),
            value,
        ))?;
        Ok(self)
    }
    /// Fire an event at the cursor; requires prior activation.
    pub fn fire_event(self, name: &str) -> Result<Self, LottieLoadError> {
        self.command(gaanim_renderer::lottie::LottieCommand::Event(
            name.to_owned(),
        ))?;
        Ok(self)
    }
    fn package_ids(&self, kind: u8) -> Vec<String> {
        let spec = self.drawable.spec.lock().expect("object spec poisoned");
        let SpawnKind::Lottie { playback } = &spec.kind else {
            unreachable!()
        };
        playback
            .package
            .as_ref()
            .map(|p| match kind {
                0 => p.package.animation_ids.clone(),
                1 => p.package.theme_ids.clone(),
                _ => p.package.state_machine_ids.clone(),
            })
            .unwrap_or_default()
    }
    /// Animation IDs in manifest order; empty for JSON.
    pub fn animation_ids(&self) -> Vec<String> {
        self.package_ids(0)
    }
    /// Theme IDs in manifest order; empty for JSON.
    pub fn theme_ids(&self) -> Vec<String> {
        self.package_ids(1)
    }
    /// State machine IDs in manifest order; empty for JSON.
    pub fn state_machine_ids(&self) -> Vec<String> {
        self.package_ids(2)
    }

    fn belongs_to(&self, state: &SharedCanvasState) -> bool {
        Arc::ptr_eq(&self.state, state)
    }

    pub fn source_width(&self) -> usize {
        self.asset.width()
    }

    pub fn source_height(&self) -> usize {
        self.asset.height()
    }

    pub fn frame_rate(&self) -> f64 {
        self.asset.frame_rate()
    }

    pub fn source_duration(&self) -> f64 {
        self.asset.duration()
    }

    pub fn warnings(&self) -> &[String] {
        self.asset.warnings()
    }
}

impl AudioClip {
    fn belongs_to(&self, state: &SharedCanvasState) -> bool {
        Arc::ptr_eq(&self.state, state)
    }
}

/// One value accepted by the mixed animation/audio playback API.
// Built once per scene or clip, not stored in bulk: boxing the large
// variant would only add indirection.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum PlayItem {
    Animation(Anim),
    Audio(AudioClip),
    Video(VideoClip),
    VideoSegment(VideoSegment),
    Lottie(LottieClip),
}

/// The channel a glTF model's animations write: the pose of its nodes.
const GLTF_POSE: &str = "gltf_pose";

/// A channel a launched animation drives until `end`.
#[derive(Clone, Debug)]
pub(crate) struct LaunchedChannel {
    target: gaanim_core::ObjectId,
    channel: String,
    start: f64,
    end: f64,
    label: &'static str,
}

/// A pure, nestable description of temporal composition.
#[derive(Debug, Clone)]
pub struct Composition {
    node: CompositionNode,
    delay: f64,
    default_duration: Option<f64>,
    default_rate: Option<RateFunc>,
    stretch: Option<f64>,
    /// Speed ramp or time remap of the whole composition (TM-06).
    time_warp: Option<gaanim_math::TimeWarpSpec>,
    /// Whole-composition repetitions and the gap between them, in seconds.
    repeat: Option<(u32, f64)>,
    /// Items placed after the children at label-relative or absolute positions.
    inserts: Vec<(Composition, InsertPosition)>,
}

#[derive(Debug, Clone)]
enum CompositionNode {
    Leaf(Box<PlayItem>),
    /// Zero-duration named instant, see [`Composition::label`].
    Label(String),
    Parallel(Vec<Composition>),
    Sequence {
        children: Vec<Composition>,
        gap: f64,
    },
    Stagger {
        children: Vec<Composition>,
        each: f64,
        layout: Option<StaggerLayout>,
    },
}

/// Where a spatial stagger starts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StaggerOrigin {
    /// The first item.
    Start,
    /// The last item.
    End,
    /// The center of the items' bounding box.
    Center,
    /// The edges of the bounding box, moving inward.
    Edges,
    /// A seeded random order.
    Random(u64),
    /// A scene point.
    Point(f64, f64),
}

/// Distance-based stagger: each child's delay grows with its distance from
/// `origin`, shaped by `easing`, spanning `total` seconds when given or
/// `each` seconds per grid step otherwise.
#[derive(Debug, Clone)]
pub struct StaggerLayout {
    pub origin: StaggerOrigin,
    /// Explicit `(rows, columns)`; `None` uses authored positions.
    pub grid: Option<(usize, usize)>,
    pub total: Option<f64>,
    pub easing: Option<RateFunc>,
}

/// Authored scene position of a drawable, folded from its declaration-time
/// placement; `None` when it depends on layout resolved at compile time.
pub(crate) fn authored_position(spec: &super::types::ObjectSpec) -> Option<DVec2> {
    use super::types::LayoutOp;
    let mut position = DVec2::ZERO;
    for op in &spec.layout_ops {
        match op {
            LayoutOp::SetTranslation(to) => position = to.truncate(),
            LayoutOp::ShiftBy(delta) => position += delta.truncate(),
            LayoutOp::MoveAnchorTo { target, .. } | LayoutOp::MoveTextAnchorTo { target, .. } => {
                position = target.truncate()
            }
            LayoutOp::SetScale(_)
            | LayoutOp::SetScale3D(_)
            | LayoutOp::ScaleBy(_)
            | LayoutOp::SetRotation(_)
            | LayoutOp::SetRotation3D(_)
            | LayoutOp::RotateBy(_)
            | LayoutOp::SetSkew(_)
            | LayoutOp::SetPivot(_) => {}
            _ => return None,
        }
    }
    Some(position)
}

/// Normalized stagger weights in `[0, 1]` (0 starts first) and the number of
/// unit steps spanned, for items at `positions`.
pub fn stagger_weights(
    positions: &[DVec2],
    origin: StaggerOrigin,
    easing: Option<&RateFunc>,
) -> (Vec<f64>, f64) {
    let count = positions.len();
    if count <= 1 {
        return (vec![0.0; count], 0.0);
    }
    let distances: Vec<f64> = match origin {
        StaggerOrigin::Random(seed) => {
            let mut order: Vec<usize> = (0..count).collect();
            gaanim_math::SeededRng::new(seed).shuffle(&mut order);
            let mut ranks = vec![0.0; count];
            for (rank, index) in order.into_iter().enumerate() {
                ranks[index] = rank as f64;
            }
            ranks
        }
        _ => {
            let (min, max) = positions.iter().fold(
                (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY)),
                |(min, max), point| (min.min(*point), max.max(*point)),
            );
            let center = (min + max) * 0.5;
            let from = match origin {
                StaggerOrigin::Start => positions[0],
                StaggerOrigin::End => positions[count - 1],
                StaggerOrigin::Point(x, y) => DVec2::new(x, y),
                _ => center,
            };
            let to_center: Vec<f64> = positions.iter().map(|p| p.distance(from)).collect();
            if matches!(origin, StaggerOrigin::Edges) {
                let far = to_center.iter().copied().fold(0.0, f64::max);
                to_center.iter().map(|distance| far - distance).collect()
            } else {
                to_center
            }
        }
    };
    let low = distances.iter().copied().fold(f64::INFINITY, f64::min);
    let distances: Vec<f64> = distances.iter().map(|distance| distance - low).collect();
    let far = distances.iter().copied().fold(0.0, f64::max);
    if far <= 1e-12 {
        return (vec![0.0; count], 0.0);
    }
    // One step is the smallest spacing between items, so a row staggered
    // from its start spans `count - 1` steps like `index * each`.
    let mut step = f64::INFINITY;
    let sample = &positions[..count.min(512)];
    for (index, a) in sample.iter().enumerate() {
        for b in &sample[index + 1..] {
            let gap = a.distance(*b);
            if gap > 1e-9 {
                step = step.min(gap);
            }
        }
    }
    let steps = if matches!(origin, StaggerOrigin::Random(_)) || !step.is_finite() {
        far
    } else {
        far / step
    };
    let weights = distances
        .iter()
        .map(|distance| {
            let normalized = distance / far;
            easing.map_or(normalized, |easing| easing.evaluate(normalized))
        })
        .collect();
    (weights, steps)
}

/// One resolved leaf returned by [`Composition::schedule`].
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleEntry {
    pub path: Vec<usize>,
    pub kind: &'static str,
    pub start: f64,
    pub duration: Option<f64>,
    pub end: Option<f64>,
}

/// Read-only local timing information for a composition.
#[derive(Debug, Clone, PartialEq)]
pub struct Schedule {
    pub entries: Vec<ScheduleEntry>,
    pub span: f64,
    /// Resolved labels in time order, in the composition's local seconds.
    pub labels: Vec<ScheduleLabel>,
}

#[derive(Debug, Clone)]
struct ResolvedPlayItem {
    item: PlayItem,
    start: f64,
    path: Vec<usize>,
    duration: Option<f64>,
    /// Inside a warped composition, where the leaf's authored seconds (from
    /// its own start) play; `start` and `duration` are then played times.
    map: Option<gaanim_math::TimeMap>,
}

impl ResolvedPlayItem {
    fn shift(&mut self, offset: f64) {
        self.start += offset;
        if let Some(map) = &mut self.map {
            map.push(gaanim_math::TimeStage::Shift(offset));
        }
    }

    /// Bends the item's time with `warp`, which starts where its
    /// composition does.
    fn warp(&mut self, warp: &gaanim_math::TimeWarp) {
        let start = self.start;
        let map = self.map.get_or_insert_with(|| {
            gaanim_math::TimeMap(vec![gaanim_math::TimeStage::Shift(start)])
        });
        map.push(gaanim_math::TimeStage::Warp(warp.clone()));
        self.retime();
    }

    /// Played start and duration from the map.
    fn retime(&mut self) {
        let Some(map) = &self.map else {
            return;
        };
        let authored = match &self.item {
            PlayItem::Animation(anim) => anim.inner.duration.max(0.0),
            _ => 0.0,
        };
        self.start = map.played(0.0);
        self.duration = self
            .duration
            .map(|_| (map.played(authored) - self.start).max(0.0));
    }
}

impl Composition {
    pub fn leaf(item: impl Into<PlayItem>) -> Self {
        Self {
            node: CompositionNode::Leaf(Box::new(item.into())),
            delay: 0.0,
            default_duration: None,
            default_rate: None,
            stretch: None,
            time_warp: None,
            repeat: None,
            inserts: Vec::new(),
        }
    }

    fn branch(node: CompositionNode) -> Result<Self, PlayError> {
        let empty = match &node {
            CompositionNode::Leaf(_) | CompositionNode::Label(_) => false,
            CompositionNode::Parallel(children)
            | CompositionNode::Sequence { children, .. }
            | CompositionNode::Stagger { children, .. } => children.is_empty(),
        };
        if empty {
            return Err(PlayError::EmptyComposition);
        }
        Ok(Self {
            node,
            delay: 0.0,
            default_duration: None,
            default_rate: None,
            stretch: None,
            time_warp: None,
            repeat: None,
            inserts: Vec::new(),
        })
    }

    pub fn parallel(children: Vec<Self>) -> Result<Self, PlayError> {
        Self::branch(CompositionNode::Parallel(children))
    }

    pub fn sequence(children: Vec<Self>, gap: f64) -> Result<Self, PlayError> {
        if !gap.is_finite() {
            return Err(PlayError::InvalidCompositionTiming("gap"));
        }
        Self::branch(CompositionNode::Sequence { children, gap })
    }

    pub fn stagger(children: Vec<Self>, each: f64) -> Result<Self, PlayError> {
        if !each.is_finite() || each < 0.0 {
            return Err(PlayError::InvalidCompositionTiming("each"));
        }
        Self::branch(CompositionNode::Stagger {
            children,
            each,
            layout: None,
        })
    }

    /// Stagger children by their distance from `layout.origin` instead of
    /// their index.
    pub fn stagger_layout(
        children: Vec<Self>,
        each: f64,
        layout: StaggerLayout,
    ) -> Result<Self, PlayError> {
        if !each.is_finite() || each < 0.0 {
            return Err(PlayError::InvalidCompositionTiming("each"));
        }
        if layout
            .total
            .is_some_and(|total| !total.is_finite() || total < 0.0)
        {
            return Err(PlayError::InvalidCompositionTiming("total"));
        }
        Self::branch(CompositionNode::Stagger {
            children,
            each,
            layout: Some(layout),
        })
    }

    pub fn delay(mut self, seconds: f64) -> Result<Self, PlayError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(PlayError::InvalidCompositionTiming("delay"));
        }
        self.delay = seconds;
        Ok(self)
    }

    pub fn defaults(
        mut self,
        duration: Option<f64>,
        rate: Option<RateFunc>,
    ) -> Result<Self, PlayError> {
        if duration.is_some_and(|value| !value.is_finite() || value < 0.0) {
            return Err(PlayError::InvalidCompositionTiming("duration"));
        }
        self.default_duration = duration;
        self.default_rate = rate;
        Ok(self)
    }

    /// Plays the whole composition `count` times, `gap` seconds apart.
    pub fn repeat(mut self, count: u32, gap: f64) -> Result<Self, PlayError> {
        if count == 0 {
            return Err(PlayError::InvalidCompositionTiming("count"));
        }
        if !gap.is_finite() || gap < 0.0 {
            return Err(PlayError::InvalidCompositionTiming("gap"));
        }
        if self.contains_media() {
            return Err(PlayError::RepeatContainsMedia);
        }
        self.repeat = Some((count, gap));
        Ok(self)
    }

    pub fn stretch(mut self, seconds: f64) -> Result<Self, PlayError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(PlayError::InvalidCompositionTiming("stretch"));
        }
        if self.contains_media() {
            return Err(PlayError::StretchContainsMedia);
        }
        self.stretch = Some(seconds);
        Ok(self)
    }

    /// Changes playback speed through the composition: `keys` are speeds
    /// at fractions of its span, joined linearly, so `[(0.4, 0.15), (0.6,
    /// 0.15)]` holds a slow motion in the middle. Its length becomes the
    /// time the ramp takes; children keep their order and overlaps.
    pub fn speed_ramp(mut self, keys: Vec<(f64, f64)>) -> Result<Self, PlayError> {
        if self.contains_media() {
            return Err(PlayError::WarpContainsMedia);
        }
        self.time_warp =
            Some(gaanim_math::TimeWarpSpec::speed_ramp(keys).map_err(PlayError::InvalidTimeWarp)?);
        Ok(self)
    }

    /// Runs the composition's time through `easing`: at a fraction `p` of
    /// its span it shows what was authored at `easing(p)`. The span keeps
    /// its length; the easing must not go back.
    pub fn time_remap(mut self, easing: RateFunc) -> Result<Self, PlayError> {
        if self.contains_media() {
            return Err(PlayError::WarpContainsMedia);
        }
        self.time_warp =
            Some(gaanim_math::TimeWarpSpec::remap(easing).map_err(PlayError::InvalidTimeWarp)?);
        Ok(self)
    }

    fn contains_media(&self) -> bool {
        let node = match &self.node {
            CompositionNode::Leaf(item) => !matches!(item.as_ref(), PlayItem::Animation(_)),
            CompositionNode::Label(_) => false,
            CompositionNode::Parallel(children)
            | CompositionNode::Sequence { children, .. }
            | CompositionNode::Stagger { children, .. } => {
                children.iter().any(Self::contains_media)
            }
        };
        node || self.inserts.iter().any(|(item, _)| item.contains_media())
    }

    fn resolve(
        &self,
        inherited_duration: Option<f64>,
        inherited_rate: Option<RateFunc>,
        path: &mut Vec<usize>,
    ) -> Result<Resolution, PlayError> {
        let duration = self.default_duration.or(inherited_duration);
        let rate = self.default_rate.clone().or(inherited_rate);
        // Extent of the last non-label child, the reference of `<` and `>`.
        let mut previous: Option<(f64, f64)> = None;
        let mut resolved = match &self.node {
            CompositionNode::Leaf(item) => {
                let mut item = item.as_ref().clone();
                if let PlayItem::Animation(anim) = &mut item {
                    anim.apply_play_defaults(duration, rate.clone());
                    anim.apply_repeat();
                    anim.apply_settle();
                }
                let (start, item_duration) = match &mut item {
                    PlayItem::Animation(anim) => {
                        let start = anim.inner.delay.max(0.0);
                        anim.inner.delay = 0.0;
                        (start, Some(anim.inner.duration.max(0.0)))
                    }
                    PlayItem::Audio(audio) => (
                        0.0,
                        audio.extends_play.then_some(audio.track.duration).flatten(),
                    ),
                    PlayItem::Video(video) => (0.0, video.duration),
                    PlayItem::VideoSegment(segment) => (0.0, Some(segment.interval.scene_end())),
                    PlayItem::Lottie(lottie) => (0.0, lottie.duration),
                };
                Resolution {
                    items: vec![ResolvedPlayItem {
                        item,
                        start,
                        path: path.clone(),
                        duration: item_duration,
                        map: None,
                    }],
                    labels: Vec::new(),
                }
            }
            CompositionNode::Label(name) => Resolution {
                items: Vec::new(),
                labels: vec![ScheduleLabel {
                    name: name.clone(),
                    time: 0.0,
                }],
            },
            CompositionNode::Parallel(children) => {
                let mut tree = Resolution::default();
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    let child_tree = child.resolve(duration, rate.clone(), path)?;
                    path.pop();
                    if !child.is_label() {
                        previous = Some(child_tree.extent().unwrap_or((0.0, 0.0)));
                    }
                    tree.absorb(child_tree)?;
                }
                tree
            }
            CompositionNode::Stagger {
                children,
                each,
                layout,
            } => {
                let offsets: Vec<f64> = match layout {
                    None => (0..children.len())
                        .map(|index| index as f64 * *each)
                        .collect(),
                    Some(layout) => {
                        let positions = Self::stagger_positions(children, layout.grid);
                        let (weights, steps) =
                            stagger_weights(&positions, layout.origin, layout.easing.as_ref());
                        let span = layout.total.unwrap_or(steps * *each);
                        weights.into_iter().map(|weight| weight * span).collect()
                    }
                };
                let mut tree = Resolution::default();
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    let mut child_tree = child.resolve(duration, rate.clone(), path)?;
                    path.pop();
                    let offset = offsets[index];
                    child_tree.shift(offset);
                    if !child.is_label() {
                        previous = Some(child_tree.extent().unwrap_or((offset, offset)));
                    }
                    tree.absorb(child_tree)?;
                }
                tree
            }
            CompositionNode::Sequence { children, gap } => {
                let mut tree = Resolution::default();
                // Labels take no step and no gap: they mark where the next
                // step starts, or where the last one ends when trailing.
                let mut pending_labels = Vec::new();
                let mut last_step: Option<(f64, f64)> = None;
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    let mut child_tree = child.resolve(duration, rate.clone(), path)?;
                    path.pop();
                    if child.is_label() {
                        pending_labels.push(child_tree);
                        continue;
                    }
                    let cursor = match last_step {
                        None => 0.0,
                        Some((start, span)) => {
                            let next = start + span + *gap;
                            if next + f64::EPSILON < start {
                                return Err(PlayError::SequenceOverlapTooLarge);
                            }
                            next.max(start)
                        }
                    };
                    for mut label in pending_labels.drain(..) {
                        label.shift(cursor);
                        tree.absorb(label)?;
                    }
                    let child_span = child_tree.span();
                    child_tree.shift(cursor);
                    previous = Some(child_tree.extent().unwrap_or((cursor, cursor)));
                    tree.absorb(child_tree)?;
                    last_step = Some((cursor, child_span));
                }
                let tail = last_step.map_or(0.0, |(start, span)| start + span);
                for mut label in pending_labels {
                    label.shift(tail);
                    tree.absorb(label)?;
                }
                tree
            }
        };
        if matches!(self.node, CompositionNode::Leaf(_)) {
            previous = resolved.extent();
        }
        let child_count = match &self.node {
            CompositionNode::Leaf(_) | CompositionNode::Label(_) => 0,
            CompositionNode::Parallel(children)
            | CompositionNode::Sequence { children, .. }
            | CompositionNode::Stagger { children, .. } => children.len(),
        };
        for (index, (item, at)) in self.inserts.iter().enumerate() {
            let time = at.resolve(&resolved.labels, previous, resolved.span())?;
            path.push(child_count + index);
            let mut tree = item.resolve(duration, rate.clone(), path)?;
            path.pop();
            tree.shift(time);
            if !item.is_label() {
                previous = Some(tree.extent().unwrap_or((time, time)));
            }
            resolved.absorb(tree)?;
        }

        if let Some(spec) = &self.time_warp {
            if resolved
                .items
                .iter()
                .any(|item| !matches!(item.item, PlayItem::Animation(_)))
            {
                return Err(PlayError::WarpContainsMedia);
            }
            let span = resolved.span();
            if span > 0.0 {
                let warp = spec.build(span);
                for item in &mut resolved.items {
                    item.warp(&warp);
                }
                for label in &mut resolved.labels {
                    label.time = warp.played(label.time);
                }
            }
        }
        if let Some(target_span) = self.stretch {
            if resolved
                .items
                .iter()
                .any(|item| !matches!(item.item, PlayItem::Animation(_)))
            {
                return Err(PlayError::StretchContainsMedia);
            }
            let current_span = resolved.span();
            if current_span == 0.0 {
                if target_span != 0.0 {
                    return Err(PlayError::CannotStretchZeroSpan);
                }
            } else {
                let factor = target_span / current_span;
                for item in &mut resolved.items {
                    if let Some(map) = &mut item.map {
                        map.push(gaanim_math::TimeStage::Scale(factor));
                        item.retime();
                        continue;
                    }
                    item.start *= factor;
                    if let PlayItem::Animation(anim) = &mut item.item {
                        anim.inner.duration *= factor;
                        item.duration = Some(anim.inner.duration);
                    }
                }
                for label in &mut resolved.labels {
                    label.time *= factor;
                }
            }
        }
        // Repetitions replay the items; labels keep their first-cycle time.
        if let Some((count, gap)) = self.repeat
            && count > 1
        {
            let span = resolved.span();
            let first = resolved.items.clone();
            for cycle in 1..count {
                let offset = cycle as f64 * (span + gap);
                resolved.items.extend(first.iter().map(|item| {
                    let mut copy = item.clone();
                    copy.shift(offset);
                    if let PlayItem::Animation(anim) = &copy.item {
                        copy.item = PlayItem::Animation(anim.replica());
                    }
                    copy
                }));
            }
        }
        resolved.shift(self.delay);
        Ok(resolved)
    }

    /// Positions used by a spatial stagger: grid cells when `grid` is given,
    /// authored positions when every child has one, else the index on a row.
    fn stagger_positions(children: &[Self], grid: Option<(usize, usize)>) -> Vec<DVec2> {
        if let Some((_, columns)) = grid {
            let columns = columns.max(1);
            return (0..children.len())
                .map(|index| DVec2::new((index % columns) as f64, -((index / columns) as f64)))
                .collect();
        }
        let authored: Option<Vec<DVec2>> = children
            .iter()
            .map(|child| child.first_animation().and_then(Anim::authored_position))
            .collect();
        authored.unwrap_or_else(|| {
            (0..children.len())
                .map(|index| DVec2::new(index as f64, 0.0))
                .collect()
        })
    }

    fn first_animation(&self) -> Option<&Anim> {
        match &self.node {
            CompositionNode::Leaf(item) => match item.as_ref() {
                PlayItem::Animation(anim) => Some(anim),
                _ => None,
            },
            CompositionNode::Label(_) => None,
            CompositionNode::Parallel(children)
            | CompositionNode::Sequence { children, .. }
            | CompositionNode::Stagger { children, .. } => {
                children.iter().find_map(Self::first_animation)
            }
        }
    }

    fn resolved(
        &self,
        duration: Option<f64>,
        rate: Option<RateFunc>,
    ) -> Result<Vec<ResolvedPlayItem>, PlayError> {
        self.resolve(duration, rate, &mut Vec::new())
            .map(|resolution| resolution.items)
    }

    pub fn schedule(&self, duration: Option<f64>) -> Result<Schedule, PlayError> {
        let Resolution {
            items: resolved,
            mut labels,
        } = self.resolve(duration, None, &mut Vec::new())?;
        labels.sort_by(|left, right| left.time.total_cmp(&right.time));
        Ok(Schedule {
            labels,
            span: resolved_span(&resolved),
            entries: resolved
                .into_iter()
                .map(|item| ScheduleEntry {
                    path: item.path,
                    kind: play_item_kind(&item.item),
                    start: item.start,
                    duration: item.duration,
                    end: item.duration.map(|duration| item.start + duration),
                })
                .collect(),
        })
    }
}

// -- Timeline labels and relative positions (TM-05) --

/// A named instant resolved by [`Composition::schedule`], in local seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleLabel {
    pub name: String,
    pub time: f64,
}

/// Where [`Composition::insert`] places an item, GSAP position style.
#[derive(Debug, Clone, PartialEq)]
pub enum InsertPosition {
    /// Absolute local time in seconds (`0.5`).
    At(f64),
    /// A label reference with an optional offset: `"name"`, `"name+0.2"`,
    /// `"name-0.2"`, `"name+=0.2"`. An exact label name always wins, so
    /// names such as `"part-2"` stay usable.
    Label(String),
    /// Start of the previous child or insert plus an offset (`"<"`, `"<+0.1"`).
    PreviousStart(f64),
    /// End of the previous child or insert plus an offset (`">"`, `">-0.2"`).
    PreviousEnd(f64),
    /// Current composition end plus an offset (`"+=0.3"`, `"-=0.3"`).
    End(f64),
}

fn invalid_position(position: &str, reason: &str) -> PlayError {
    PlayError::InvalidInsertPosition(format!("{position:?}: {reason}"))
}

/// Signed offset such as `+0.2`, `-0.2`, `+=0.2` or `-=0.2`; empty is zero.
fn parse_position_offset(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    let (sign, rest) = match text.as_bytes()[0] {
        b'+' => (1.0, &text[1..]),
        b'-' => (-1.0, &text[1..]),
        _ => return None,
    };
    let rest = rest.strip_prefix('=').unwrap_or(rest).trim();
    if rest.starts_with(['+', '-']) {
        return None;
    }
    rest.parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| sign * value)
}

impl InsertPosition {
    /// An absolute local time; it must be finite and non-negative.
    pub fn at(seconds: f64) -> Result<Self, PlayError> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(invalid_position(
                &seconds.to_string(),
                "an absolute position must be a finite, non-negative number of seconds",
            ));
        }
        Ok(Self::At(seconds))
    }

    /// Parse a GSAP-style position string. Label names are checked when the
    /// composition is scheduled.
    pub fn parse(text: &str) -> Result<Self, PlayError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(invalid_position(text, "a position must not be empty"));
        }
        if let Some(rest) = trimmed.strip_prefix('<') {
            return parse_position_offset(rest)
                .map(Self::PreviousStart)
                .ok_or_else(|| {
                    invalid_position(
                        text,
                        "expected \"<\" or \"<\" plus an offset such as \"<+0.1\"",
                    )
                });
        }
        if let Some(rest) = trimmed.strip_prefix('>') {
            return parse_position_offset(rest)
                .map(Self::PreviousEnd)
                .ok_or_else(|| {
                    invalid_position(
                        text,
                        "expected \">\" or \">\" plus an offset such as \">-0.2\"",
                    )
                });
        }
        if trimmed.starts_with("+=") || trimmed.starts_with("-=") {
            return parse_position_offset(trimmed)
                .map(Self::End)
                .ok_or_else(|| invalid_position(text, "expected \"+=seconds\" or \"-=seconds\""));
        }
        if trimmed.starts_with(['+', '-', '=']) {
            return Err(invalid_position(
                text,
                "offsets relative to the end need \"+=seconds\" or \"-=seconds\"; \
                 expected a label, \"<\", \">\" or seconds otherwise",
            ));
        }
        if let Ok(seconds) = trimmed.parse::<f64>() {
            if !seconds.is_finite() {
                return Err(invalid_position(
                    text,
                    "an absolute position must be a finite number of seconds",
                ));
            }
            return Ok(Self::At(seconds));
        }
        Ok(Self::Label(trimmed.to_owned()))
    }

    /// Absolute local time given the labels resolved so far, the extent of
    /// the previous child or insert and the current span.
    fn resolve(
        &self,
        labels: &[ScheduleLabel],
        previous: Option<(f64, f64)>,
        span: f64,
    ) -> Result<f64, PlayError> {
        let time = match self {
            Self::At(seconds) => *seconds,
            Self::PreviousStart(offset) => previous.map_or(0.0, |(start, _)| start) + offset,
            Self::PreviousEnd(offset) => previous.map_or(0.0, |(_, end)| end) + offset,
            Self::End(offset) => span + offset,
            Self::Label(text) => Self::resolve_label(text, labels)?,
        };
        if time < -1e-9 {
            return Err(invalid_position(
                &self.describe(),
                &format!("resolves to {time:.3}s, before the composition start"),
            ));
        }
        Ok(time.max(0.0))
    }

    fn resolve_label(text: &str, labels: &[ScheduleLabel]) -> Result<f64, PlayError> {
        let find = |name: &str| labels.iter().find(|label| label.name == name);
        if let Some(label) = find(text) {
            return Ok(label.time);
        }
        let mut name = text;
        for (index, character) in text.char_indices().skip(1) {
            if !matches!(character, '+' | '-') {
                continue;
            }
            let (head, tail) = text.split_at(index);
            if head.trim().is_empty() {
                continue;
            }
            if let Some(offset) = parse_position_offset(tail) {
                name = head.trim();
                if let Some(label) = find(name) {
                    return Ok(label.time + offset);
                }
                break;
            }
        }
        let mut known: Vec<&str> = labels.iter().map(|label| label.name.as_str()).collect();
        known.sort_unstable();
        Err(PlayError::UnknownLabel {
            name: name.to_owned(),
            known: if known.is_empty() {
                "none".to_owned()
            } else {
                known
                    .iter()
                    .map(|name| format!("{name:?}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
        })
    }

    fn describe(&self) -> String {
        let signed = |offset: f64| {
            if offset == 0.0 {
                String::new()
            } else if offset > 0.0 {
                format!("+{offset}")
            } else {
                offset.to_string()
            }
        };
        match self {
            Self::At(seconds) => seconds.to_string(),
            Self::Label(text) => text.clone(),
            Self::PreviousStart(offset) => format!("<{}", signed(*offset)),
            Self::PreviousEnd(offset) => format!(">{}", signed(*offset)),
            Self::End(offset) if *offset < 0.0 => format!("-={}", -offset),
            Self::End(offset) => format!("+={offset}"),
        }
    }
}

/// Resolved leaves and labels of one composition subtree.
#[derive(Debug, Clone, Default)]
struct Resolution {
    items: Vec<ResolvedPlayItem>,
    labels: Vec<ScheduleLabel>,
}

impl Resolution {
    fn shift(&mut self, offset: f64) {
        self.items.iter_mut().for_each(|item| item.shift(offset));
        self.labels
            .iter_mut()
            .for_each(|label| label.time += offset);
    }

    fn span(&self) -> f64 {
        resolved_span(&self.items)
    }

    /// First start and last end of the resolved leaves.
    fn extent(&self) -> Option<(f64, f64)> {
        let start = self
            .items
            .iter()
            .map(|item| item.start)
            .min_by(f64::total_cmp)?;
        let end = self
            .items
            .iter()
            .map(|item| item.start + item.duration.unwrap_or(0.0))
            .fold(start, f64::max);
        Some((start, end))
    }

    fn absorb(&mut self, other: Resolution) -> Result<(), PlayError> {
        for label in &other.labels {
            if self.labels.iter().any(|known| known.name == label.name) {
                return Err(PlayError::DuplicateLabel(label.name.clone()));
            }
        }
        self.items.extend(other.items);
        self.labels.extend(other.labels);
        Ok(())
    }
}

impl Composition {
    /// A zero-duration named instant for `sequence`/`parallel`/`stagger`.
    ///
    /// In a sequence a label takes no step and no gap: it marks where the
    /// next step starts, or where the last one ends when it is trailing.
    pub fn label(name: impl Into<String>) -> Result<Self, PlayError> {
        let name = name.into().trim().to_owned();
        if name.is_empty() {
            return Err(PlayError::EmptyLabel);
        }
        if name.parse::<f64>().is_ok() || name.starts_with(['<', '>', '+', '-', '=']) {
            return Err(PlayError::InvalidLabel(name));
        }
        Ok(Self {
            node: CompositionNode::Label(name),
            delay: 0.0,
            default_duration: None,
            default_rate: None,
            stretch: None,
            time_warp: None,
            repeat: None,
            inserts: Vec::new(),
        })
    }

    /// Place `item` at `at`, resolved against this composition's labels and
    /// children when it is scheduled. Inserted labels become referable by
    /// later inserts; `<`/`>` refer to the most recent non-label insert, or
    /// the last child before any insert.
    pub fn insert(mut self, item: Composition, at: InsertPosition) -> Result<Self, PlayError> {
        if self.repeat.is_some() && item.contains_media() {
            return Err(PlayError::RepeatContainsMedia);
        }
        if self.stretch.is_some() && item.contains_media() {
            return Err(PlayError::StretchContainsMedia);
        }
        self.inserts.push((item, at));
        Ok(self)
    }

    fn is_label(&self) -> bool {
        matches!(self.node, CompositionNode::Label(_)) && self.inserts.is_empty()
    }
}

/// Validate solid-arrow dimensions and scale the head, keeping its
/// proportions, so its length is at most `max_head_ratio` of `length`.
fn capped_arrow_head(
    length: f64,
    head_length: f64,
    head_width: f64,
    body_width: f64,
    max_head_ratio: Option<f64>,
) -> Result<(f64, f64), &'static str> {
    if ![head_length, head_width, body_width]
        .iter()
        .all(|v| v.is_finite() && *v > 0.0)
    {
        return Err("arrow dimensions must be finite and positive");
    }
    if max_head_ratio.is_some_and(|v| !v.is_finite() || v <= 0.0 || v > 1.0) {
        return Err("max_head_ratio must be in (0, 1]");
    }
    if !length.is_finite() {
        return Err("arrow length must be finite");
    }
    let factor = max_head_ratio.map_or(1.0, |ratio| (length * ratio / head_length).min(1.0));
    Ok((head_length * factor, head_width * factor))
}

fn play_item_kind(item: &PlayItem) -> &'static str {
    match item {
        PlayItem::Animation(_) => "animation",
        PlayItem::Audio(_) => "audio",
        PlayItem::Video(_) => "video",
        PlayItem::VideoSegment(_) => "video_segment",
        PlayItem::Lottie(_) => "lottie",
    }
}

fn resolved_span(items: &[ResolvedPlayItem]) -> f64 {
    items
        .iter()
        .map(|item| item.start + item.duration.unwrap_or(0.0))
        .fold(0.0, f64::max)
}

impl From<Anim> for PlayItem {
    fn from(value: Anim) -> Self {
        Self::Animation(value)
    }
}

impl From<AudioClip> for PlayItem {
    fn from(value: AudioClip) -> Self {
        Self::Audio(value)
    }
}

impl From<VideoClip> for PlayItem {
    fn from(value: VideoClip) -> Self {
        Self::Video(value)
    }
}

impl From<VideoSegment> for PlayItem {
    fn from(value: VideoSegment) -> Self {
        Self::VideoSegment(value)
    }
}

impl From<LottieClip> for PlayItem {
    fn from(value: LottieClip) -> Self {
        Self::Lottie(value)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StopLoopError {
    #[error(transparent)]
    Stop(#[from] SegmentError),
    #[error(transparent)]
    Play(#[from] PlayError),
    #[error("a stop's loop must last longer than zero seconds")]
    Empty,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlayError {
    #[error(
        "cannot mix whole-video playback and segments on the same Video; create a separate Video"
    )]
    MixedVideoPlayback,
    #[error("video segments overlap on the same Video")]
    OverlappingVideoSegments,
    #[error("VideoSegment already scheduled; create another segment to repeat")]
    VideoSegmentConsumed,
    #[error("invalid paint animation: {0}")]
    InvalidPaint(String),
    #[error(
        "channel '{channel}' on {target:?} is reactively bound; animate its Parameter or assign a fixed value first"
    )]
    BoundProperty {
        target: gaanim_core::ObjectId,
        channel: String,
    },
    #[error("invalid custom animation: {0}")]
    CustomAnimation(String),
    #[error("animations can only be played by their owning Scene")]
    ForeignAnimation,
    #[error("repeat() applies to animations only; repeat media by scheduling it again")]
    RepeatContainsMedia,
    #[error("an Anim can only be played once")]
    AnimationAlreadyConsumed,
    #[error("the same Anim cannot appear twice in one play call")]
    DuplicateAnimation,
    #[error("an animation proxy must select at least one action or property")]
    EmptyAnimation,
    #[error(
        "play overlaps {first} and {second} on the same {target_kind} ({target}); both drive its \
         '{channel}' channel. Play them one after another{}",
        combine_hint(.combinable)
    )]
    ConflictingChannel {
        target: gaanim_core::ObjectId,
        channel: String,
        /// Drawable kind, such as `line` or `circle`.
        target_kind: &'static str,
        /// The earlier animation and its span within the call, e.g. `Trim (0.00–1.00 s)`.
        first: String,
        second: String,
        /// Both only set properties, so one animation can set them together.
        combinable: bool,
    },
    #[error("audio declarations can only be played by their owning Scene")]
    ForeignAudio,
    #[error("video declarations can only be played by their owning Scene")]
    ForeignVideo,
    #[error("a video declaration can only be activated once")]
    VideoAlreadyActivated,
    #[error("Lottie declarations can only be played by their owning Scene")]
    ForeignLottie,
    #[error("a Lottie declaration can only be activated once")]
    LottieAlreadyActivated,
    #[error("a composition must contain at least one item")]
    EmptyComposition,
    #[error("composition {0} must be finite and valid")]
    InvalidCompositionTiming(&'static str),
    #[error("a negative sequence gap cannot start a step before the previous step")]
    SequenceOverlapTooLarge,
    #[error("invalid animation sound: {0}")]
    InvalidSound(String),
    #[error("stretch() cannot contain Audio, Video, or Lottie leaves")]
    StretchContainsMedia,
    #[error("a zero-span composition can only be stretched to zero seconds")]
    CannotStretchZeroSpan,
    #[error("speed_ramp() and time_remap() cannot contain Audio, Video, or Lottie leaves")]
    WarpContainsMedia,
    #[error("{0}")]
    InvalidTimeWarp(String),
    #[error("label names must not be empty")]
    EmptyLabel,
    #[error(
        "invalid label name {0:?}: labels cannot be numbers or start with '<', '>', '+', '-' or '='"
    )]
    InvalidLabel(String),
    #[error("label {0:?} is defined more than once in the same composition")]
    DuplicateLabel(String),
    #[error("unknown label {name:?}; defined labels: {known}")]
    UnknownLabel { name: String, known: String },
    #[error("invalid insert position {0}")]
    InvalidInsertPosition(String),
}

fn combine_hint(combinable: &bool) -> &'static str {
    if *combinable {
        " or combine them into one animation"
    } else {
        ""
    }
}

/// Whether `anim`, through its timeline `channel`, would take over `bound`,
/// a channel a reactive binding owns. A bound opacity is a factor of its
/// own that entrances and exits such as `fade_in` multiply, so only a fixed
/// opacity target takes it over. Another bound channel excludes its own
/// animations and effects that may move the drawable, but not the draw
/// animations, which only reveal it.
fn writes_bound_channel(
    anim: &Anim,
    channel: &str,
    bound: gaanim_animation::PropertyChannel,
) -> bool {
    use crate::anim::AnimationType::*;
    if bound == gaanim_animation::PropertyChannel::Opacity {
        return channel == "opacity"
            && !matches!(
                anim.inner.anim_type,
                FadeIn
                    | FadeOut
                    | FadeInFrom { .. }
                    | Create3D
                    | ReplacementTransform { .. }
                    | MagicMove { .. }
            );
    }
    channel == bound.name()
        || (channel == "effect"
            && !matches!(
                anim.inner.anim_type,
                Write { .. }
                    | Create { .. }
                    | Unwrite { .. }
                    | Uncreate { .. }
                    | DrawBorderThenFill { .. }
            ))
}

fn animation_channels(anim: &Anim) -> Vec<String> {
    use crate::anim::AnimationType::*;
    if let PropertySource(source) = &anim.inner.anim_type {
        return vec![source.sources.channel().name().to_owned()];
    }
    if matches!(anim.inner.anim_type, FadeInFrom { .. }) {
        return vec!["translation".into(), "opacity".into()];
    }
    if let TextAnimator(spec) = &anim.inner.anim_type {
        return spec.channels();
    }
    if let CustomProperties(animation) = &anim.inner.anim_type {
        return animation
            .channels()
            .iter()
            .map(|channel| channel.timeline_channel().to_owned())
            .collect();
    }
    if let Motion(motion) = &anim.inner.anim_type {
        return motion
            .channels()
            .iter()
            .map(|channel| channel.timeline_channel().to_owned())
            .collect();
    }
    let properties = match &anim.inner.anim_type {
        Properties(properties) | TextSelectionProperties { properties, .. } => Some(properties),
        _ => None,
    };
    if let Some(properties) = properties {
        let prefix = match &anim.inner.anim_type {
            TextSelectionProperties {
                fragment,
                occurrence,
                ..
            } => {
                format!("text:{fragment}:{occurrence:?}:")
            }
            _ => String::new(),
        };
        let mut channels = Vec::new();
        for (present, name) in [
            (
                properties.translation.is_some()
                    || matches!(
                        properties.rotation,
                        Some(crate::anim::PropertyRotation::By2D { pivot: Some(_), .. })
                    ),
                "translation",
            ),
            (properties.rotation.is_some(), "rotation"),
            (properties.scale.is_some(), "scale"),
            (properties.skew.is_some(), "skew"),
            (properties.opacity.is_some(), "opacity"),
            (
                properties.fill.is_some() || properties.visible_color.is_some(),
                "fill",
            ),
            (
                properties.stroke_color.is_some() || properties.visible_color.is_some(),
                "stroke_color",
            ),
            (properties.stroke_width.is_some(), "stroke_width"),
            (properties.material.is_some(), "material"),
            (properties.fill_level.is_some(), "fill_level"),
            (properties.media_frame.is_some(), "media_frame"),
            (properties.glow.is_some(), "glow"),
            (properties.blur.is_some(), "blur"),
            (properties.shadow.is_some(), "shadow"),
            (!properties.morph.is_empty(), "morph"),
        ] {
            if present {
                channels.push(format!("{prefix}{name}"));
            }
        }
        channels.extend(
            properties
                .source_targets
                .iter()
                .map(|target| target.sources.channel().name().to_owned()),
        );
        return channels;
    }
    let compound_channels: &[&str] = match &anim.inner.anim_type {
        FadeInFrom { .. } => &["translation", "opacity"],
        RotateBy { pivot: Some(_), .. } => &["rotation", "translation"],
        GrowFromPoint { .. } | GrowFromEdge { .. } | CameraViewPop { .. } => {
            &["translation", "scale"]
        }
        SpinInFromNothing => &["scale", "rotation"],
        Create3D => &["scale", "opacity"],
        Indicate { .. } => &["scale", "fill"],
        CameraFollowPath {
            follow:
                crate::anim::PathFollowOptions {
                    orient: Some(_), ..
                },
            ..
        } => &["camera_pose", "camera_rotation"],
        CameraDollyZoom { .. } => &["camera_pose", "camera_projection"],
        // A transform keeps the source's own opacity, so it can fade at once.
        Transform { .. } => &[
            "translation",
            "rotation",
            "scale",
            "fill",
            "stroke_color",
            "stroke_width",
            "effect",
        ],
        ReplacementTransform { .. } | MagicMove { .. } => &[
            "translation",
            "rotation",
            "scale",
            "fill",
            "stroke_color",
            "stroke_width",
            "opacity",
            "effect",
        ],
        _ => &[],
    };
    if !compound_channels.is_empty() {
        return compound_channels
            .iter()
            .map(|channel| (*channel).to_owned())
            .collect();
    }
    let channel = match &anim.inner.anim_type {
        TranslateTo { .. }
        | TranslateAnchorTo { .. }
        | TranslateToAnchorPoint { .. }
        | TranslateBy { .. }
        | MoveAlongPath { .. }
        | MoveAlongPath3D { .. }
        | Wiggle { .. } => "translation",
        RotateTo { .. } | RotateBy { .. } | RotateBy3D { .. } => "rotation",
        ScaleTo { .. }
        | ScaleUniform { .. }
        | ScaleBy3D { .. }
        | GrowFromCenter
        | ShrinkToCenter
        | CameraViewZoomTo { .. } => "scale",
        SkewTo { .. } => "skew",
        FadeTo { .. } | FadeIn | FadeOut | FadeInFrom { .. } => "opacity",
        FillColorTo { .. } | FillPaintTo { .. } => "fill",
        StrokeColorTo { .. } | StrokePaintTo { .. } => "stroke_color",
        StrokeWidthTo { .. } => "stroke_width",
        Material3DTo { .. } => "material",
        MediaFrameTo { .. } => "media_frame",
        SignalFloat { .. } | SignalKeyframes(_) => "signal",
        CameraPosition { .. }
        | CameraPositionSource { .. }
        | CameraFrame { .. }
        | CameraFrameMany { .. }
        | CameraFollow { .. }
        | CameraFollowEndpoint { .. }
        | CameraLookAt { .. }
        | CameraLookAtSource { .. }
        | CameraOrbit { .. }
        | CameraDolly { .. }
        | CameraFollowPath { .. }
        | CameraWhipPan { .. }
        | CameraState { .. }
        | CameraReset => "camera_pose",
        CameraZoom { .. }
        | CameraZoomSource { .. }
        | CameraOrthographic { .. }
        | CameraPerspective { .. } => "camera_projection",
        CameraRotation { .. } | CameraRotationSource { .. } => "camera_rotation",
        CameraShake { .. } => "camera_shake",
        CameraDepthOfField { .. } => "camera_depth_of_field",
        CameraLensShift { .. } => "camera_lens_shift",
        TextPathOffset { .. } => "text_path",
        GltfAnimation { .. } => GLTF_POSE,
        _ => "effect",
    };
    vec![channel.to_owned()]
}

/// Default length in scene units for the straight segments at either spring end.
pub const DEFAULT_SPRING_STRAIGHT: f64 = 12.0;

/// Default nominal font size for labels, values, and units in reactive annotations.
pub const DEFAULT_REACTIVE_TEXT_SIZE: f64 = 0.48;

/// Why a 3D line between endpoints could not be made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Line3DError {
    #[error(transparent)]
    Mesh(#[from] primitives3d::Primitive3DError),
    #[error("3D line endpoints must be finite and belong to this Scene")]
    InvalidEndpoint,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum BooleanError {
    #[error("boolean operations require at least two vector drawables")]
    TooFewOperands,
    #[error("boolean operands must belong to this Scene")]
    ForeignScene,
    #[error("boolean tolerance must be finite and positive")]
    InvalidTolerance,
    #[error("fill level must be finite and between zero and one")]
    InvalidFillLevel,
    #[error("boolean operands must be closed 2D vector drawables")]
    NonVectorOperand,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum SurroundingRectError {
    #[error("surrounding_rect requires at least one target")]
    NoTargets,
    #[error("padding must contain finite non-negative values")]
    InvalidPadding,
    #[error("corner_radius must be finite and non-negative")]
    InvalidCornerRadius,
}

/// A live bounds frame with a typed retarget operation.
#[derive(Debug, Clone)]
pub struct SurroundingRectHandle {
    pub drawable: DrawableHandle,
    targets: Arc<Mutex<Vec<BoundsTarget>>>,
}

impl SurroundingRectHandle {
    pub fn retarget(
        &self,
        targets: Vec<BoundsTarget>,
        duration: Option<f64>,
    ) -> Result<Anim, SurroundingRectError> {
        if targets.is_empty() {
            return Err(SurroundingRectError::NoTargets);
        }
        let from = {
            let mut current = self
                .targets
                .lock()
                .expect("surrounding rect targets poisoned");
            let from = current.clone();
            current.clone_from(&targets);
            from
        };
        Ok(self
            .drawable
            .surrounding_rect_retarget(from, targets, duration))
    }
}

/// Public pieces of a reactive technical dimension.
#[derive(Debug, Clone)]
pub struct DimensionHandle {
    pub drawable: DrawableHandle,
    pub line: DrawableHandle,
    pub extensions: DrawableHandle,
    pub label: Option<DrawableHandle>,
    pub number: Option<DrawableHandle>,
    pub unit: Option<DrawableHandle>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DimensionExtensionStyle {
    #[default]
    Solid,
    Dashed,
}

/// Public pieces of a reactive angular dimension.
#[derive(Debug, Clone)]
pub struct AngleDimensionHandle {
    pub drawable: DrawableHandle,
    pub arc: DrawableHandle,
    pub arrows: DrawableHandle,
    pub extensions: DrawableHandle,
    pub label: Option<DrawableHandle>,
    pub number: Option<DrawableHandle>,
    pub unit: Option<DrawableHandle>,
}

/// Optional annotation and geometry behavior for a reactive angular dimension.
#[derive(Debug, Clone)]
pub struct AngleDimensionOptions {
    pub label: Option<String>,
    pub show_value: bool,
    pub format: String,
    pub unit: String,
    pub sweep: gaanim_animation::AngleSweep,
    pub arrowheads: gaanim_animation::AngleArrowheads,
    pub label_gap: f64,
    pub label_orientation: gaanim_animation::DimensionLabelOrientation,
    pub show_extensions: bool,
    pub font_size: Option<f64>,
    pub color: Option<Color>,
}

impl Default for AngleDimensionOptions {
    fn default() -> Self {
        Self {
            label: None,
            show_value: false,
            format: ".1f".to_owned(),
            unit: "deg".to_owned(),
            sweep: gaanim_animation::AngleSweep::Minor,
            arrowheads: gaanim_animation::AngleArrowheads::Both,
            label_gap: 0.12,
            label_orientation: gaanim_animation::DimensionLabelOrientation::Upright,
            show_extensions: true,
            font_size: None,
            color: None,
        }
    }
}

/// Public, independently styleable pieces of a mechanical support symbol.
#[derive(Debug, Clone)]
pub struct SupportHandle {
    pub drawable: DrawableHandle,
    pub joint: DrawableHandle,
    pub body: DrawableHandle,
    pub ground: DrawableHandle,
    pub rollers: DrawableHandle,
    pub guides: DrawableHandle,
    pub hatching: DrawableHandle,
}

/// Public, independently styleable pieces of a reactive force vector.
#[derive(Debug, Clone)]
pub struct ForceVectorHandle {
    pub drawable: DrawableHandle,
    pub shaft: DrawableHandle,
    pub head: DrawableHandle,
    pub label: Option<DrawableHandle>,
    pub number: Option<DrawableHandle>,
    pub unit: Option<DrawableHandle>,
}

/// A persistent native camera constraint authored on the scene timeline.
#[derive(Clone)]
pub struct CameraConstraintHandle {
    spec: SharedCameraBindingSpec,
    state: SharedCanvasState,
}

/// How far a camera in an orbit shot stands from what it looks at.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrbitReach {
    /// This far, in world units.
    Distance(f64),
    /// As far as shows this many scene units per world unit across the
    /// plane through the target (see [`gaanim_math::orbit_distance`]).
    Scale(f64),
}

/// Reusable complete camera state owned by one scene.
#[derive(Clone)]
pub struct CameraStateHandle {
    source: gaanim_animation::CameraStateSource,
    state: SharedCanvasState,
}

impl std::fmt::Debug for CameraStateHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CameraStateHandle")
            .field("source", &self.source)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CameraStateError {
    #[error("camera state names must not be empty")]
    EmptyName,
    #[error("unknown camera state '{0}'")]
    UnknownName(String),
    #[error("camera states can only be used with their owning Scene")]
    ForeignScene,
    #[error(
        "a captured or saved camera state is only known when the scene plays;          project with a state from state_2d or state_3d, or follow camera.screen_point"
    )]
    NotAuthored,
}

impl std::fmt::Debug for CameraConstraintHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CameraConstraintHandle").finish()
    }
}

impl CameraConstraintHandle {
    /// Switch this constraint on or off at the current timeline cursor. Like
    /// a cut, a switch made after a stop at its instant comes after it, so
    /// the stop still shows the camera as it was.
    fn switch(&self, enabled: bool) {
        let order = self.spec.lock().expect("camera binding poisoned").order;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SwitchCameraBinding { order, enabled });
    }

    /// Enable this constraint at the current timeline cursor.
    pub fn enable(&self) {
        self.switch(true);
    }

    /// Disable this constraint at the current timeline cursor.
    pub fn disable(&self) {
        self.switch(false);
    }

    /// Let this constraint go as the next segment starts, so it holds for
    /// the segment it was made in, and for that segment alone while a
    /// transition shows it leave.
    pub fn until_segment_end(&self) {
        let order = self.spec.lock().expect("camera binding poisoned").order;
        let mut state = self.state.lock().expect("canvas state poisoned");
        if !state.segment_camera_bindings.contains(&order) {
            state.segment_camera_bindings.push(order);
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CameraBindingError {
    #[error("camera binding must control at least one channel")]
    Empty,
    #[error("camera up vector must be finite and non-zero")]
    InvalidUp,
    #[error("camera influence must be finite and within 0..1")]
    InvalidInfluence,
    #[error("orthographic zoom must be finite and greater than zero")]
    InvalidZoom,
    #[error("perspective fov_y must be finite and satisfy 0 < fov_y < pi")]
    InvalidFov,
    #[error("2D camera centers cannot contain a non-zero z coordinate")]
    InvalidDimension,
}

/// Optional annotation behavior for [`SceneModel::dimension_between_with_options`].
#[derive(Debug, Clone)]
pub struct DimensionOptions {
    pub label: Option<String>,
    pub show_value: bool,
    /// Optional semantic value shown by the readout. When present it implies
    /// `show_value` and takes precedence over measured distance and `scale`.
    pub value: Option<ScalarSource>,
    pub format: String,
    pub unit: Option<String>,
    pub scale: f64,
    pub label_gap: f64,
    pub label_orientation: gaanim_animation::DimensionLabelOrientation,
    /// Scene side of the dimension. When set, only the magnitude of the
    /// offset is used, whatever the `from` → `to` direction.
    pub side: Option<gaanim_animation::DimensionSide>,
    pub font_size: Option<f64>,
    /// Typography of the annotation (label, value and unit). Unset fields use
    /// the theme's body text; `font_size` overrides its size and its color
    /// overrides `color` for the text.
    pub label_style: gaanim_text::prelude::TextStyle,
    pub color: Option<Color>,
    pub line_width: f64,
    pub extension_style: DimensionExtensionStyle,
    pub dash_length: f64,
    pub gap_length: f64,
}

impl Default for DimensionOptions {
    fn default() -> Self {
        Self {
            label: None,
            show_value: false,
            value: None,
            format: ".2f".to_owned(),
            unit: None,
            scale: 1.0,
            label_gap: 0.10,
            label_orientation: gaanim_animation::DimensionLabelOrientation::Upright,
            side: None,
            font_size: Some(DEFAULT_REACTIVE_TEXT_SIZE),
            label_style: gaanim_text::prelude::TextStyle::default(),
            color: None,
            line_width: 0.03,
            extension_style: DimensionExtensionStyle::Solid,
            dash_length: 0.12,
            gap_length: 0.08,
        }
    }
}

/// Error returned when selecting a built-in visual theme by name.
#[derive(Debug, thiserror::Error)]
#[error("unknown theme '{name}'; use CanvasTheme::BUILTIN_NAMES for the available color schemes")]
pub struct ThemeError {
    pub name: String,
}

/// Failures while decoding a raster image requested by `SceneModel::image`.
#[derive(Debug, thiserror::Error)]
pub enum ImageLoadError {
    #[error("could not load image '{path}': {source}")]
    Load {
        path: PathBuf,
        #[source]
        source: image::ImageError,
    },
    #[error(transparent)]
    Options(#[from] ImageOptionsError),
}

/// Failures while validating or opening a video requested by `SceneModel::video`.
#[derive(Debug, thiserror::Error)]
pub enum VideoLoadError {
    #[error(transparent)]
    Media(#[from] gaanim_media::VideoError),
    #[error(transparent)]
    Options(#[from] ImageOptionsError),
    #[error(transparent)]
    Audio(#[from] AudioTrackError),
    #[error("{name} must be a finite {requirement} number")]
    InvalidNumber {
        name: &'static str,
        requirement: &'static str,
    },
    #[error("video offset must be before the end of the source")]
    OffsetOutOfRange,
    #[error("video duration extends beyond the end of the source")]
    DurationOutOfRange,
    #[error(transparent)]
    Frames(#[from] gaanim_media::frames::FramesError),
}

/// Failures while loading or configuring a Lottie JSON composition.
#[derive(Debug, thiserror::Error)]
pub enum LottieLoadError {
    #[error(transparent)]
    Lottie(#[from] gaanim_renderer::lottie::LottieError),
    #[error(transparent)]
    Options(#[from] ImageOptionsError),
    #[error("Lottie dimensions exceed the supported range")]
    DimensionsOutOfRange,
}

impl LottieLoadError {
    pub fn is_value_error(&self) -> bool {
        matches!(
            self,
            Self::Options(_)
                | Self::DimensionsOutOfRange
                | Self::Lottie(
                    gaanim_renderer::lottie::LottieError::Package(_)
                        | gaanim_renderer::lottie::LottieError::InvalidOffset
                        | gaanim_renderer::lottie::LottieError::InvalidDuration
                        | gaanim_renderer::lottie::LottieError::InvalidSpeed
                )
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AssetRootError {
    #[error("asset directory '{path}' does not exist or is not a directory")]
    Invalid { path: PathBuf },
    #[error("could not resolve asset directory '{path}': {source}")]
    Resolve {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Failures while loading Typst source from an asset file.
#[derive(Debug, thiserror::Error)]
pub enum TypstAssetError {
    #[error("could not read Typst asset '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Failures while loading a code block's tmTheme file.
#[derive(Debug, thiserror::Error)]
pub enum CodeThemeError {
    #[error("could not read code theme '{path}': {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("code theme '{path}' is {message}")]
    Invalid { path: PathBuf, message: String },
}

/// Error returned when a scene-lifetime operation receives an incompatible drawable.
#[derive(Debug, thiserror::Error)]
pub enum SceneObjectError {
    #[error("at least one drawable is required")]
    NoObjects,
    #[error("drawable belongs to a different Scene")]
    ForeignScene,
    #[error(transparent)]
    Layout(#[from] gaanim_layout::LayoutError),
}

#[derive(Clone, Copy)]
enum SceneObjectAction {
    Reuse,
    Persist,
    Release,
}

#[derive(Debug, thiserror::Error)]
pub enum AssetPreloadError {
    #[error("could not preload image '{path}': {source}")]
    Image {
        path: PathBuf,
        // Boxed: an image error is large enough on Windows to trip
        // `clippy::result_large_err`.
        #[source]
        source: Box<ImageLoadError>,
    },
    #[error("could not preload SVG '{path}': {source}")]
    Svg {
        path: PathBuf,
        #[source]
        source: SvgLoadError,
    },
    #[error("could not preload Lottie JSON '{path}': {source}")]
    Lottie {
        path: PathBuf,
        #[source]
        source: gaanim_renderer::lottie::LottieError,
    },
}

/// Process-local decoded texture cache. Each canvas still receives its own
/// mobject, while repeated references to the same canonical path share the
/// immutable RGBA data used by Vello.
static IMAGE_CACHE: OnceLock<Mutex<ImageCache>> = OnceLock::new();

/// Decoded bytes the image cache keeps at most: the images used most
/// recently stay, so a session that goes through many images does not keep
/// them all.
const IMAGE_CACHE_BYTES: usize = 512 << 20;

/// Decoded images by canonical path, with when each was last used.
#[derive(Default)]
struct ImageCache {
    entries: HashMap<PathBuf, (gaanim_core::peniko::ImageData, u64)>,
    clock: u64,
    bytes: usize,
}

impl ImageCache {
    fn get(&mut self, path: &Path) -> Option<gaanim_core::peniko::ImageData> {
        self.clock += 1;
        let clock = self.clock;
        self.entries.get_mut(path).map(|(image, used)| {
            *used = clock;
            image.clone()
        })
    }

    /// Store `image`, first forgetting the least recently used images that
    /// would take the cache past `budget` bytes.
    fn insert(
        &mut self,
        path: PathBuf,
        image: gaanim_core::peniko::ImageData,
        budget: usize,
    ) -> gaanim_core::peniko::ImageData {
        if let Some((cached, _)) = self.entries.get(&path) {
            return cached.clone();
        }
        let size = image.data.len();
        while self.bytes + size > budget
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(path, _)| path.clone())
        {
            if let Some((evicted, _)) = self.entries.remove(&oldest) {
                self.bytes -= evicted.data.len();
            }
        }
        self.clock += 1;
        self.bytes += size;
        self.entries.insert(path, (image.clone(), self.clock));
        image
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// Forget the images decoded from the files `changed`.
    fn forget(&mut self, changed: &gaanim_renderer::lottie::ChangedFiles) {
        let bytes = &mut self.bytes;
        self.entries.retain(|path, (image, _)| {
            let keep = !changed.contains(path);
            if !keep {
                *bytes -= image.data.len();
            }
            keep
        });
    }
}

/// Drop every process-local cache of files read by scenes (raster images,
/// Lottie, and Typst layouts) so the next compile reads them
/// from disk again.
pub fn clear_asset_caches() {
    if let Some(cache) = IMAGE_CACHE.get() {
        cache.lock().expect("image cache poisoned").clear();
    }
    gaanim_renderer::lottie::clear_lottie_cache();
    gaanim_text::typst_compiler::clear_typst_layout_cache();
}

/// Forget what the process-local caches hold from the files `changed` (raster
/// images, Lottie JSON and the images it references, dotLottie packages), so
/// the next compile reads them from disk again. Typst layouts stay: a
/// document's files are read while authoring, and are part of its layout's
/// key. Anything else the scene holds from a file was read while authoring,
/// and reaches the replay's fingerprints: a new image is a new allocation,
/// the rest is content.
pub fn forget_assets(changed: &[PathBuf]) {
    let changed = gaanim_renderer::lottie::ChangedFiles::new(changed);
    if let Some(cache) = IMAGE_CACHE.get() {
        cache.lock().expect("image cache poisoned").forget(&changed);
    }
    gaanim_renderer::lottie::forget_lottie_files(&changed);
}

fn load_image(path: impl AsRef<Path>) -> Result<gaanim_core::peniko::ImageData, ImageLoadError> {
    let requested = path.as_ref();
    let cache_key = requested
        .canonicalize()
        .unwrap_or_else(|_| requested.to_path_buf());
    let cache = IMAGE_CACHE.get_or_init(Mutex::default);
    if let Some(image) = cache.lock().expect("image cache poisoned").get(&cache_key) {
        return Ok(image);
    }

    let decoded = image::open(&cache_key).map_err(|source| ImageLoadError::Load {
        path: requested.to_path_buf(),
        source,
    })?;
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    let image = gaanim_core::peniko::ImageData {
        data: gaanim_core::peniko::Blob::from(rgba.into_raw()),
        format: gaanim_core::peniko::ImageFormat::Rgba8,
        alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
        width,
        height,
    };
    let mut cache = cache.lock().expect("image cache poisoned");
    Ok(cache.insert(cache_key, image, IMAGE_CACHE_BYTES))
}

/// A background that covers the scene frame with the image at `path`,
/// centered and cropped like CSS `background-size: cover`.
pub fn background_image(
    path: impl AsRef<Path>,
) -> Result<gaanim_renderer::background::BackgroundPaint, ImageLoadError> {
    let image = load_image(path)?;
    Ok(gaanim_renderer::background::BackgroundPaint::Brush(
        gaanim_core::peniko::Brush::Image(gaanim_core::peniko::ImageBrush::new(image)),
    ))
}

/// The image at `path` for an overlay post-process pass, downscaled to
/// [`gaanim_renderer::post_presets::MAX_OVERLAY_SIDE`] texels on its long
/// side when it is larger.
pub fn image_overlay(
    path: impl AsRef<Path>,
) -> Result<gaanim_renderer::post_presets::ImageOverlay, String> {
    use gaanim_renderer::post_presets::{ImageOverlay, MAX_OVERLAY_SIDE};
    let image = load_image(path).map_err(|error| error.to_string())?;
    let (width, height) = (image.width, image.height);
    let longest = width.max(height);
    if longest <= MAX_OVERLAY_SIDE {
        return ImageOverlay::from_rgba8(width, height, image.data.data())
            .map_err(|error| error.to_string());
    }
    let scale = f64::from(MAX_OVERLAY_SIDE) / f64::from(longest);
    let side =
        |length: u32| ((f64::from(length) * scale).round() as u32).clamp(1, MAX_OVERLAY_SIDE);
    let (small_width, small_height) = (side(width), side(height));
    let texels = image::RgbaImage::from_raw(width, height, image.data.data().to_vec())
        .ok_or_else(|| "the decoded image does not match its size".to_string())?;
    let small = image::imageops::resize(
        &texels,
        small_width,
        small_height,
        image::imageops::FilterType::Triangle,
    );
    ImageOverlay::from_rgba8(small_width, small_height, small.as_raw())
        .map_err(|error| error.to_string())
}

/// Default height of a presentation brand logo in scene units.
const BRAND_LOGO_HEIGHT: f64 = 0.6;
/// Inset of the brand footer and rule from the safe frame's sides.
const BRAND_INSET: f64 = 0.4;
/// Scale of the brand footer text when it fits.
const BRAND_FOOTER_SCALE: f64 = 0.5;

/// Spawn a drawable into the canvas that owns `state`, as
/// [`SceneModel::spawn`] does, so handles can create helper drawables.
pub(crate) fn spawn_in(
    state: &SharedCanvasState,
    kind: SpawnKind,
    register_top_level: bool,
) -> DrawableHandle {
    let mut guard = state.lock().expect("canvas state poisoned");
    let id = guard.next_object_id();
    let active_idx = guard.active_idx;
    if register_top_level {
        guard.active_mut().mobject_ids.push(id);
        guard.all_drawables.push(id);
    }
    drop(guard);

    let handle = DrawableHandle::new(id, kind, state.clone(), active_idx);
    let stack = if super::profile::tracks_script_lines() {
        super::profile::call_stack()
    } else {
        Vec::new()
    };
    {
        let mut state = state.lock().expect("canvas state poisoned");
        if !stack.is_empty() {
            state.object_locations.insert(id, stack);
        }
        state.object_specs.insert(id, handle.spec.clone());
        state.segments[active_idx]
            .ops
            .push(Op::Spawn(handle.spec.clone()));
    }
    handle
}

/// A live frame around `targets`, in the scene `state` belongs to.
pub(crate) fn surrounding_rect_in(
    state: &SharedCanvasState,
    targets: Vec<BoundsTarget>,
    padding: [f64; 4],
    corner_radius: f64,
) -> Result<SurroundingRectHandle, SurroundingRectError> {
    if targets.is_empty() {
        return Err(SurroundingRectError::NoTargets);
    }
    if padding
        .iter()
        .any(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(SurroundingRectError::InvalidPadding);
    }
    if !corner_radius.is_finite() || corner_radius < 0.0 {
        return Err(SurroundingRectError::InvalidCornerRadius);
    }
    let drawable = spawn_in(state, SpawnKind::SurroundingRect, true).no_fill();
    let id = drawable.id;
    state
        .lock()
        .expect("canvas state poisoned")
        .active_mut()
        .ops
        .push(Op::AttachSurroundingRect {
            target: id,
            sources: targets.clone(),
            padding,
            corner_radius,
        });
    Ok(SurroundingRectHandle {
        drawable,
        targets: Arc::new(Mutex::new(targets)),
    })
}

/// Top-level facade for building Gaanim animations.
#[derive(Debug, Clone)]
pub struct SceneModel {
    pub frame: SceneFrame,
    pub background: Option<Color>,
    /// Full scene-bounds paint. `background` remains the representative color used
    /// for theme contrast and the window clear.
    pub background_paint: Option<gaanim_renderer::background::BackgroundPaint>,
    pub(crate) background_overridden: bool,
    /// WGSL post-process passes applied in order to the rendered 2D scene
    /// inside the camera frame; empty for none.
    pub post_process: Vec<gaanim_renderer::post_process::PostProcessPass>,
    /// Motion blur of exported frames and snapshots; `None` for sharp frames.
    pub motion_blur: Option<gaanim_renderer::effects::MotionBlur>,
    /// Canonical name of the selected theme.
    pub theme: Option<String>,
    /// Complete semantic colors and typography for the selected theme.
    pub theme_style: Option<CanvasTheme>,
    /// Direct prose-family override applied after the active theme.
    pub(crate) font_family_override: Option<String>,
    /// Direct math-family override applied after the active theme.
    pub(crate) math_font_family_override: Option<String>,
    /// Direct code-family override applied after the active theme.
    pub(crate) code_font_family_override: Option<String>,
    pub margin: Margin,
    /// Frame height in design pixels: layout lengths written as `"16px"`
    /// convert with it, so measurements copied from a 1080p design match.
    pub design_resolution: f64,
    pub asset_root: Option<PathBuf>,
    /// Folders a relative path is looked up in when no asset root is set
    /// and the path is not in the working directory: the script's `assets`
    /// folder and the script's own folder.
    pub asset_search: Vec<PathBuf>,
    /// The workspace's shared asset folders: where a relative path missing
    /// from the scene's own folders is looked up last.
    pub shared_asset_roots: Vec<PathBuf>,
    /// Audio sources synchronized in preview and mixed by FFmpeg during export.
    pub audio_tracks: Vec<AudioTrack>,
    /// Index in `audio_tracks` of each segment's transition sound, so that a
    /// later `link` that replaces the transition also replaces its sound.
    pub(crate) transition_sounds: Vec<(super::SegmentId, usize)>,
    /// Musical tempo set with [`Self::set_tempo`], for beat-timed cuts and
    /// the editor's bar lines.
    pub(crate) tempo: Option<gaanim_timeline::timeline::BeatGrid>,
    /// Channels driven by launched animations (`play(advance=False)`), with
    /// their absolute span, so a later play cannot drive them at once.
    pub(crate) launched_channels: Vec<LaunchedChannel>,
    /// Timeline instant of a playback bundle's cover image, set with
    /// [`Self::set_thumbnail`].
    pub(crate) thumbnail_time: Option<f64>,
    /// Voiceover blocks and the live take, for timing and the editor recorder.
    pub(crate) narration: super::narration::NarrationState,
    /// Reusable logo/footer treatment generated for every explicit segment.
    pub branding: Option<PresentationBrand>,
    /// Seconds the whole talk is planned to last when presenting.
    pub talk_target: Option<f64>,
    pub(crate) camera_position: gaanim_core::glam::DVec3,
    /// The world's up axis, `DVec3::Y` (the default) or `DVec3::Z`: the 3D
    /// camera's default up and orbits, the axis of cylinders, cones, tori
    /// and planes, and where the fixed key light shines from.
    pub world_up: gaanim_core::glam::DVec3,
    pub(crate) lighting_3d: gaanim_scene::Lighting3D,
    pub(crate) transparency_3d: gaanim_scene::Transparency3D,
    pub(crate) state: SharedCanvasState,
    /// The compilation that [`Self::bounds_of`] measures in.
    pub(crate) measured: super::measure::MeasureCache,
    /// What authoring this scene cost; see [`Self::measurement_warning`].
    pub(crate) profile: super::profile::SharedProfile,
    /// Where a probe of the scene found the endpoints of camera holds that
    /// only the run time moves, by hold and endpoint (see
    /// `runtime::with_run_time_holds`).
    pub(crate) camera_hold_probes: Arc<HashMap<(usize, u8), gaanim_core::glam::DVec3>>,
}

impl SceneModel {
    /// Create a scene of `width` x `height` logical units using the default
    /// [`CanvasTheme::DEFAULT_NAME`] theme and its background.
    ///
    /// Explicit backgrounds, [`SceneModel::set_theme`] and
    /// [`SceneModel::apply_theme`] replace these defaults;
    /// [`SceneModel::clear_theme`] returns to a plain unthemed canvas.
    pub fn new(width: impl Into<f64>, height: impl Into<f64>) -> Self {
        let mut scene = Self::unthemed(width, height);
        scene.apply_theme(CanvasTheme::default_builtin());
        scene
    }

    fn unthemed(width: impl Into<f64>, height: impl Into<f64>) -> Self {
        let width = width.into();
        let height = height.into();
        Self {
            frame: SceneFrame::new(width, height),
            background: None,
            background_paint: None,
            background_overridden: false,
            post_process: Vec::new(),
            motion_blur: None,
            theme: None,
            theme_style: None,
            font_family_override: None,
            math_font_family_override: None,
            code_font_family_override: None,
            margin: Margin::default(),
            design_resolution: 1080.0,
            asset_root: None,
            asset_search: Vec::new(),
            shared_asset_roots: Vec::new(),
            audio_tracks: Vec::new(),
            transition_sounds: Vec::new(),
            tempo: None,
            launched_channels: Vec::new(),
            thumbnail_time: None,
            narration: Default::default(),
            branding: None,
            talk_target: None,
            camera_position: gaanim_core::glam::DVec3::ZERO,
            world_up: gaanim_core::glam::DVec3::Y,
            lighting_3d: gaanim_scene::Lighting3D::default(),
            transparency_3d: gaanim_scene::Transparency3D::default(),
            state: Arc::new(Authored::new(CanvasState::new())),
            measured: Default::default(),
            camera_hold_probes: Default::default(),
            profile: Default::default(),
        }
    }

    /// Whether a handle was created by this exact Scene, independent of its
    /// numeric object ID.
    pub fn owns_drawable(&self, drawable: &DrawableHandle) -> bool {
        Arc::ptr_eq(&self.state, &drawable.state)
    }

    /// Whether the scene draws 3D content (meshes, surfaces, 3D lines).
    pub fn has_3d_content(&self) -> bool {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .flat_map(|segment| segment.ops.iter())
            .any(|op| {
                matches!(
                    op,
                    Op::Spawn(spec)
                        if matches!(
                            spec.lock().expect("object spec poisoned").kind,
                            SpawnKind::Primitive3D(..)
                                | SpawnKind::SurfaceMesh { .. }
                                | SpawnKind::Polyline3D { .. }
                                | SpawnKind::ReactivePolyline3D { .. }
                                | SpawnKind::LineSegments3D { .. }
                                | SpawnKind::TracedPath3DLine
                        )
                )
            })
    }

    pub fn background(mut self, c: Color) -> Self {
        self.set_background(Some(c));
        self
    }

    pub fn set_background(&mut self, color: Option<Color>) {
        self.background = color;
        self.background_paint = color.map(gaanim_renderer::background::BackgroundPaint::solid);
        self.background_overridden = true;
    }

    /// Use any native Vello brush as the full scene background.
    pub fn background_brush(mut self, brush: Brush) -> Self {
        self.set_background_paint(Some(gaanim_renderer::background::BackgroundPaint::Brush(
            brush,
        )));
        self
    }

    /// Use a validated timeline-driven WGSL function as the full scene background.
    pub fn background_shader(
        mut self,
        source: impl Into<Arc<str>>,
        fallback: Color,
    ) -> Result<Self, gaanim_renderer::background::ShaderBackgroundError> {
        let shader = gaanim_renderer::background::ShaderBackground::new(source, fallback)?;
        self.set_background_paint(Some(gaanim_renderer::background::BackgroundPaint::Shader(
            shader,
        )));
        Ok(self)
    }

    /// Replace the scene paint while retaining a representative fallback color.
    pub fn set_background_paint(
        &mut self,
        paint: Option<gaanim_renderer::background::BackgroundPaint>,
    ) {
        self.background = paint.as_ref().map(|paint| paint.fallback_color());
        self.background_paint = paint;
        self.background_overridden = true;
    }

    /// Blur exported frames and snapshots over their shutter, or `None` for
    /// sharp frames. The interactive preview always stays sharp.
    pub fn set_motion_blur(&mut self, blur: Option<gaanim_renderer::effects::MotionBlur>) {
        self.motion_blur = blur;
    }

    /// Replace the scene post-process chain; an empty chain removes it.
    ///
    /// Uniform sources must read parameters, time or computed values of this
    /// scene.
    pub fn set_post_process(
        &mut self,
        passes: Vec<gaanim_renderer::post_process::PostProcessPass>,
    ) -> Result<(), String> {
        self.validate_post_process_sources(&passes)?;
        self.post_process = passes;
        Ok(())
    }

    fn validate_post_process_sources(
        &self,
        passes: &[gaanim_renderer::post_process::PostProcessPass],
    ) -> Result<(), String> {
        for source in passes.iter().flat_map(|pass| &pass.values) {
            if let gaanim_animation::ScalarSource::Constant(value) = source
                && !value.is_finite()
            {
                return Err("post-process uniforms must be finite".into());
            }
            let foreign = {
                let state = self.state.lock().expect("canvas state poisoned");
                source
                    .parameter_ids()
                    .iter()
                    .any(|id| !state.parameter_values.contains_key(id))
            };
            if foreign {
                return Err("post-process uniform parameters must belong to this scene".into());
            }
            if let gaanim_animation::ScalarSource::Function(function) = source {
                self.validate_reactive_function_owner(function)
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    /// Override the post-process of one segment of this scene.
    pub fn set_segment_post_process(
        &mut self,
        segment: &SegmentHandle,
        post: gaanim_renderer::post_process::PostProcessOverride,
    ) -> Result<(), SegmentError> {
        if !segment.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        if let gaanim_renderer::post_process::PostProcessOverride::Passes(passes) = &post {
            self.validate_post_process_sources(passes)
                .map_err(SegmentError::InvalidPostProcess)?;
        }
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let target = guard
            .segments
            .iter_mut()
            .find(|candidate| candidate.id == segment.id)
            .ok_or(SegmentError::UnknownSegment { id: segment.id })?;
        target.post_process = post;
        Ok(())
    }

    /// Plan `target` seconds of a presentation for one segment of this
    /// scene, or none: Presenter View counts the time spent on it against
    /// the plan.
    pub fn set_segment_target(
        &mut self,
        segment: &SegmentHandle,
        target: Option<f64>,
    ) -> Result<(), SegmentError> {
        if !segment.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let target = super::segment::segment_target(target)?;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard
            .segments
            .iter_mut()
            .find(|candidate| candidate.id == segment.id)
            .ok_or(SegmentError::UnknownSegment { id: segment.id })?
            .target = target;
        Ok(())
    }

    /// Override the motion blur of one segment of this scene: `None` keeps
    /// it sharp, while the scene's blur applies to segments without one.
    pub fn set_segment_motion_blur(
        &mut self,
        segment: &SegmentHandle,
        blur: Option<gaanim_renderer::effects::MotionBlur>,
    ) -> Result<(), SegmentError> {
        if !segment.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let target = guard
            .segments
            .iter_mut()
            .find(|candidate| candidate.id == segment.id)
            .ok_or(SegmentError::UnknownSegment { id: segment.id })?;
        target.motion_blur = Some(blur);
        Ok(())
    }

    /// Play one segment of this scene in stepped time, `rate` steps a
    /// second like stop-motion: every seek inside it shows the world at the
    /// step it falls in, counted from the segment's start and again from
    /// each of its stops, so a stop and the segment's end show their exact
    /// frame. Animations, updaters, the camera, video, particles and the
    /// time of shader backgrounds and post-processing all step; audio and
    /// the playhead do not. `None` plays the segment smoothly.
    pub fn set_segment_step_rate(
        &mut self,
        segment: &SegmentHandle,
        rate: Option<f64>,
    ) -> Result<(), SegmentError> {
        if !segment.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let rate = super::segment::segment_step_rate(rate)?;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let target = guard
            .segments
            .iter_mut()
            .find(|candidate| candidate.id == segment.id)
            .ok_or(SegmentError::UnknownSegment { id: segment.id })?;
        target.step_rate = rate;
        Ok(())
    }

    /// Apply one of the built-in visual themes.
    ///
    /// `technical` is the quiet dark style used by the built-in technical
    /// components. `presentation` adds a warmer, higher-contrast hierarchy for
    /// projected slides. `paper` provides a light documentation canvas.
    /// Calling this method also selects the theme background unless a
    /// background was set explicitly; callers can still override
    /// [`SceneModel::background`] afterwards. New scenes start with
    /// [`CanvasTheme::DEFAULT_NAME`].
    pub fn set_theme(&mut self, name: &str) -> Result<(), ThemeError> {
        self.apply_theme(CanvasTheme::builtin(name)?);
        Ok(())
    }

    /// Remove the active theme, leaving a plain unthemed canvas.
    ///
    /// Unless a background was set explicitly, the theme background is removed
    /// too and the scene renders on white. Unstyled text, shapes and axes then
    /// use the host text configuration's white foreground, so give them
    /// explicit colors or a contrasting background.
    pub fn clear_theme(&mut self) {
        if !self.background_overridden {
            self.background = None;
            self.background_paint = None;
        }
        self.theme = None;
        self.theme_style = None;
    }

    /// Apply a complete custom or derived visual theme.
    pub fn apply_theme(&mut self, theme: CanvasTheme) {
        if !self.background_overridden {
            self.background = Some(theme.palette.background);
            self.background_paint = Some(gaanim_renderer::background::BackgroundPaint::solid(
                theme.palette.background,
            ));
        }
        self.theme = Some(theme.name.clone());
        self.theme_style = Some(theme);
    }

    /// Override the canvas-wide prose, math, and code font families without
    /// creating a theme.
    ///
    /// Each supplied family replaces the corresponding default or themed
    /// family. Omitted families retain their current override, and explicit
    /// per-text font options still have higher priority.
    pub fn set_fonts(
        &mut self,
        font: Option<String>,
        math_font: Option<String>,
        code_font: Option<String>,
    ) -> Result<(), String> {
        for (name, family) in [
            ("font", font.as_deref()),
            ("math_font", math_font.as_deref()),
            ("code_font", code_font.as_deref()),
        ] {
            if family.is_some_and(|family| family.trim().is_empty()) {
                return Err(format!("{name} must not be empty"));
            }
        }
        if let Some(font) = font {
            self.font_family_override = Some(font);
        }
        if let Some(math_font) = math_font {
            self.math_font_family_override = Some(math_font);
        }
        if let Some(code_font) = code_font {
            self.code_font_family_override = Some(code_font);
        }
        Ok(())
    }

    /// Typst source defining `theme`, a dictionary of the active theme's
    /// colors by token (`theme.accent`, `theme.foreground`…), that Typst
    /// documents and the packages they import can be styled with. Without a
    /// theme it holds the background and the body text color.
    pub(crate) fn typst_theme_prelude(&self, body: Color) -> String {
        fn typst_rgb(color: Color) -> String {
            let rgba = color.to_rgba8();
            if rgba.a == u8::MAX {
                format!("rgb(\"#{:02x}{:02x}{:02x}\")", rgba.r, rgba.g, rgba.b)
            } else {
                format!(
                    "rgb(\"#{:02x}{:02x}{:02x}{:02x}\")",
                    rgba.r, rgba.g, rgba.b, rgba.a
                )
            }
        }
        let mut colors: Vec<(String, Color)> = match &self.theme_style {
            Some(theme) => theme
                .colors
                .iter()
                .map(|(token, color)| (token.clone(), *color))
                .collect(),
            None => vec![
                (
                    "background".to_owned(),
                    self.background.unwrap_or(Color::WHITE),
                ),
                ("foreground".to_owned(), body),
            ],
        };
        // Tokens that are Typst identifiers, in a fixed order.
        colors.retain(|(token, _)| {
            token.starts_with(|c: char| c.is_ascii_alphabetic())
                && token
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        });
        colors.sort_by(|a, b| a.0.cmp(&b.0));
        let entries = colors
            .iter()
            .map(|(token, color)| format!("{token}: {}", typst_rgb(*color)))
            .collect::<Vec<_>>()
            .join(", ");
        if entries.is_empty() {
            return "#let theme = (:)\n".to_owned();
        }
        format!("#let theme = ({entries},)\n")
    }

    /// Resolve a semantic token from the active theme.
    /// The categorical palette of the active theme (`gaanim` without one).
    pub fn categorical_palette(&self) -> gaanim_core::Palette {
        self.theme_style
            .as_ref()
            .map(|theme| theme.categorical.clone())
            .unwrap_or_default()
    }

    pub fn theme_color(&self, role: &str) -> Result<Color, String> {
        self.theme_style
            .as_ref()
            .ok_or_else(|| "no theme is active on this canvas".to_string())?
            .color(role)
    }

    /// Resolve a spacing/layout token. Scenes without an active theme use the
    /// canonical default token scale so templates remain deterministic.
    pub fn theme_layout_token(&self, name: &str) -> Result<f64, String> {
        self.theme_style
            .as_ref()
            .map(|theme| theme.layout.clone())
            .unwrap_or_default()
            .get(name)
    }

    /// Validate the active theme for projected-text readability.
    pub fn validate_theme(&self) -> Result<Vec<String>, String> {
        Ok(self
            .theme_style
            .as_ref()
            .ok_or_else(|| "no theme is active on this canvas".to_string())?
            .validate())
    }

    pub(crate) fn themed_text_config(&self) -> gaanim_text::prelude::TextConfig {
        self.scene_text_config(&gaanim_text::prelude::TextConfig::default())
    }

    /// Text configuration this scene compiles with: the active theme's roles,
    /// or `base` (the host configuration) without a theme, followed by the
    /// canvas font overrides. Runtime replay, fingerprints and
    /// [`SceneModel::compile`] all resolve text through this method.
    pub(crate) fn scene_text_config(
        &self,
        base: &gaanim_text::prelude::TextConfig,
    ) -> gaanim_text::prelude::TextConfig {
        let mut config = self
            .theme_style
            .as_ref()
            .map_or_else(|| base.clone(), |theme| theme.text.clone());
        if let Some(theme) = &self.theme_style {
            for (role, overlay) in &theme.text_styles {
                if let Some(style) = config.roles.get_mut(role) {
                    if let Some(font) = &overlay.font {
                        style.font_family = font.clone();
                    }
                    if let Some(size) = overlay.size {
                        style.size = size;
                    }
                    if let Some(color) = overlay.color {
                        style.fill_color = color;
                    }
                }
            }
        }
        if let Some(font) = &self.font_family_override {
            for role in [
                TextRole::Title,
                TextRole::Subtitle,
                TextRole::Kicker,
                TextRole::Heading,
                TextRole::Body,
                TextRole::Caption,
                TextRole::Label,
            ] {
                config.roles.get_mut(&role).unwrap().font_family = font.clone();
            }
        }
        if let Some(math_font) = &self.math_font_family_override {
            config.roles.get_mut(&TextRole::Math).unwrap().font_family = math_font.clone();
        }
        if let Some(code_font) = &self.code_font_family_override {
            config.roles.get_mut(&TextRole::Code).unwrap().font_family = code_font.clone();
        }
        config
    }

    /// Markup mode for text that does not choose one: the active theme's
    /// `text_markup`, or markup enabled without a theme.
    pub fn default_text_markup(&self) -> bool {
        self.theme_style
            .as_ref()
            .is_none_or(|theme| theme.text_markup)
    }

    pub(crate) fn register_theme_fonts(&self, registry: &mut gaanim_text::font::FontRegistry) {
        if let Some(theme) = &self.theme_style {
            for font in &theme.fonts {
                registry.register_font_bytes(font.family.clone(), font.bytes.clone());
            }
        }
    }

    /// Measure laid-out text without spawning it, through the exact pipeline
    /// that renders `scene.text`: role defaults from the active theme, Typst
    /// shaping, and the shared Typst hierarchy cache. Returns
    /// `(width, height)` in scene units.
    ///
    /// `wrap_width` composes the text at a fixed line width (like
    /// `TextFlow` wrap); `None` measures a single unwrapped block. Explicit
    /// `size`, `font`, and `color` overrides resolve exactly as they would on
    /// the spawned text object.
    pub fn measure_text(
        &self,
        content: &str,
        role: Option<gaanim_text::prelude::TextRole>,
        size: Option<f64>,
        font: Option<String>,
        color: Option<Color>,
        wrap_width: Option<f64>,
    ) -> Result<(f64, f64), String> {
        use gaanim_text::prelude::{
            TextContent, TextFlow, TextRole, TextSpec, TextStyle, TextWrap,
        };

        let style = TextStyle {
            size,
            font,
            color,
            ..TextStyle::default()
        };
        let flow = TextFlow {
            wrap: match wrap_width {
                Some(width) => TextWrap::Width(width.max(1.0e-6)),
                None => TextWrap::NoWrap,
            },
            ..TextFlow::default()
        };
        let spec = TextSpec::new(
            vec![TextContent::Literal(content.to_string())],
            Some(role.unwrap_or(TextRole::Body)),
            style,
            flow,
        )
        .map_err(|error| error.to_string())?;
        self.measure_text_spec(&spec)
    }

    /// Measure a complete text spec (role, style, flow and markup mode) as it
    /// would render, without spawning it. Returns `(width, height)` in scene
    /// units; `TextWrap::Auto` measures unwrapped because no layout width is
    /// offered.
    pub fn measure_text_spec(
        &self,
        spec: &gaanim_text::prelude::TextSpec,
    ) -> Result<(f64, f64), String> {
        use gaanim_text::prelude::TextRole;

        let config = self.themed_text_config();
        let role_style = config
            .roles
            .get(&spec.role)
            .ok_or_else(|| format!("no style configured for text role {:?}", spec.role))?;
        let math_font = spec.style.math_font.clone().unwrap_or_else(|| {
            config
                .roles
                .get(&TextRole::Math)
                .map(|style| style.font_family.clone())
                .unwrap_or_else(|| "New Computer Modern Math".to_string())
        });

        let font_size = spec.style.size.unwrap_or(role_style.size).max(1.0e-6);
        let font_family = spec
            .style
            .font
            .clone()
            .unwrap_or_else(|| role_style.font_family.clone());
        let color = spec.style.color.unwrap_or(role_style.fill_color);

        // Typst measurement reads only registered fonts; skip the system scan.
        let mut registry = gaanim_text::font::FontRegistry::without_system_fonts();
        self.register_theme_fonts(&mut registry);
        let source = crate::canvas::compile::fitted_text_typst_source(
            &registry,
            spec,
            None,
            font_size,
            &font_family,
            &math_font,
            color,
        );

        let bounds = gaanim_text::prelude::measure_typst(
            &registry,
            &source,
            false,
            Some(&font_family),
            Some(&math_font),
            Some(font_size),
            None,
            Some(Brush::Solid(color)),
            gaanim_scene::StrokeBrush::transparent(),
        )
        .map_err(|errors| errors.join("; "))?;

        Ok((bounds.width().max(0.0), bounds.height().max(0.0)))
    }

    pub fn with_frame(mut self, frame: SceneFrame) -> Self {
        self.frame = frame;
        self
    }

    /// Set uniform margin on all four sides.
    /// Layout operations (`to_edge`, `to_corner`) will respect this inset.
    pub fn margin_all(mut self, v: f64) -> Self {
        self.margin = Margin::all(v);
        self
    }

    /// Set per-side margins.
    pub fn margin(mut self, m: Margin) -> Self {
        self.margin = m;
        self
    }

    /// Set the base directory used by relative asset paths.
    pub fn set_asset_root(&mut self, path: impl AsRef<Path>) -> Result<(), AssetRootError> {
        let path = path.as_ref().to_path_buf();
        if !path.is_dir() {
            return Err(AssetRootError::Invalid { path });
        }
        self.asset_root = Some(
            path.canonicalize()
                .map_err(|source| AssetRootError::Resolve {
                    path: path.clone(),
                    source,
                })?,
        );
        Ok(())
    }

    /// Where a relative asset path is: in the asset root, or without one in
    /// the working directory and then [`Self::asset_search`]; then in the
    /// workspace's [`Self::shared_asset_roots`]. Missing everywhere, the
    /// first place, so the error names it.
    pub(crate) fn resolve_asset_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            return path.to_path_buf();
        }
        let first = match &self.asset_root {
            Some(root) => root.join(path),
            None => path.to_path_buf(),
        };
        if first.exists() {
            return first;
        }
        let search = if self.asset_root.is_some() {
            &[][..]
        } else {
            &self.asset_search[..]
        };
        search
            .iter()
            .chain(&self.shared_asset_roots)
            .map(|folder| folder.join(path))
            .find(|candidate| candidate.exists())
            .unwrap_or(first)
    }

    /// Declare an audio source without scheduling it.
    ///
    /// Pass the returned clip to [`Self::play_items`] to activate it at that
    /// play call's absolute timeline cursor.
    pub fn audio(
        &self,
        path: impl AsRef<Path>,
        duration: Option<f64>,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Result<AudioClip, AudioTrackError> {
        let track = AudioTrack::new(
            self.resolve_asset_path(path),
            0.0,
            duration,
            volume,
            fade_in,
            fade_out,
        )?;
        Ok(AudioClip {
            track,
            state: self.state.clone(),
            extends_play: true,
            plays: Default::default(),
        })
    }

    /// Declare background audio trimmed at `end` seconds of the file.
    ///
    /// Unlike a `duration`, `end` does not lengthen the play that activates
    /// it: the track plays under what follows, is cut at `end` and can fade
    /// out before it.
    pub fn audio_until(
        &self,
        path: impl AsRef<Path>,
        end: f64,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Result<AudioClip, AudioTrackError> {
        let track = AudioTrack::new(
            self.resolve_asset_path(path),
            0.0,
            Some(end),
            volume,
            fade_in,
            fade_out,
        )?;
        Ok(AudioClip {
            track,
            state: self.state.clone(),
            extends_play: false,
            plays: Default::default(),
        })
    }

    /// Schedule a sound effect at the absolute timeline second `at` (the
    /// current cursor when `None`).
    ///
    /// The sound plays its whole file once and never lengthens the timeline.
    /// Unlike [`Self::audio`], it needs no `play`: it is placed immediately.
    pub fn sfx(
        &mut self,
        path: impl AsRef<Path>,
        at: Option<f64>,
        volume: f64,
    ) -> Result<(), AudioTrackError> {
        let start = at.unwrap_or_else(|| self.current_time());
        let track = AudioTrack::new(self.resolve_asset_path(path), start, None, volume, 0.0, 0.0)?;
        self.audio_tracks.push(track);
        Ok(())
    }

    /// Resolve `cue` into a track for an event that starts at `event_start`.
    fn sound_track(
        &self,
        cue: &gaanim_timeline::sound::SoundCue,
        event_start: f64,
    ) -> Result<AudioTrack, AudioTrackError> {
        AudioTrack::new(
            self.resolve_asset_path(&cue.path),
            cue.start_for(event_start),
            None,
            cue.volume,
            0.0,
            0.0,
        )
    }

    /// Place (or replace) the sound of the transition entering `segment`,
    /// which starts at `segment_start`.
    fn set_transition_sound(&mut self, segment: super::SegmentId, track: Option<AudioTrack>) {
        if let Some(position) = self
            .transition_sounds
            .iter()
            .position(|(id, _)| *id == segment)
        {
            let (_, index) = self.transition_sounds.remove(position);
            self.audio_tracks.remove(index);
            for (_, other) in &mut self.transition_sounds {
                if *other > index {
                    *other -= 1;
                }
            }
        }
        if let Some(track) = track {
            self.transition_sounds
                .push((segment, self.audio_tracks.len()));
            self.audio_tracks.push(track);
        }
    }

    /// Resolve and validate assets before playback. Raster images and Lottie
    /// compositions are also decoded into their process-local caches.
    pub fn preload(&self, paths: &[PathBuf]) -> Result<(), AssetPreloadError> {
        for path in paths {
            let resolved = self.resolve_asset_path(path);
            let extension = resolved
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default();
            if extension.eq_ignore_ascii_case("json") || extension.eq_ignore_ascii_case("lottie") {
                gaanim_renderer::lottie::LottieAsset::load(&resolved).map_err(|source| {
                    AssetPreloadError::Lottie {
                        path: resolved.clone(),
                        source,
                    }
                })?;
            } else if extension.eq_ignore_ascii_case("svg") {
                gaanim_objects::prelude::SvgDocument::load(&resolved).map_err(|source| {
                    AssetPreloadError::Svg {
                        path: resolved.clone(),
                        source,
                    }
                })?;
            } else {
                load_image(&resolved).map_err(|source| AssetPreloadError::Image {
                    path: resolved.clone(),
                    source: Box::new(source),
                })?;
            }
        }
        Ok(())
    }

    /// Drop cached raster, Lottie, and glTF assets so the next load observes
    /// files changed on disk. SVG documents are resolved anew for every drawable.
    pub fn reload_assets(&mut self) {
        clear_asset_caches();
    }

    /// Scene units per design pixel.
    pub fn pixel_unit(&self) -> f64 {
        self.frame.bounds().height() / self.design_resolution.max(1.0)
    }

    pub fn safe_frame(&self) -> gaanim_math::Bounds3D {
        let raw = self.frame.bounds();
        gaanim_math::Bounds3D::new_2d(
            raw.min.x + self.margin.left,
            raw.min.y + self.margin.bottom,
            raw.max.x - self.margin.right,
            raw.max.y - self.margin.top,
        )
    }

    pub fn current_time(&self) -> f64 {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .map(|segment| segment.cursor)
            .sum()
    }

    pub fn segment_count(&self) -> usize {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len()
    }

    /// Return whether an authored handle belongs to this canvas.
    #[doc(hidden)]
    pub fn owns(&self, handle: &DrawableHandle) -> bool {
        Arc::ptr_eq(&self.state, &handle.state)
    }

    pub(crate) fn spawn(&mut self, kind: SpawnKind) -> DrawableHandle {
        self.spawn_registered(kind, true)
    }

    fn spawn_registered(&mut self, kind: SpawnKind, register_top_level: bool) -> DrawableHandle {
        spawn_in(&self.state, kind, register_top_level)
    }

    // -- Segment management --

    /// Create a named segment and switch to it.
    ///
    /// The first explicit segment replaces the untouched implicit segment. A
    /// transition therefore requires an existing authored predecessor.
    pub fn segment(
        &mut self,
        name: impl Into<String>,
        transition: Option<TransitionType>,
    ) -> Result<SegmentHandle, SegmentError> {
        self.segment_with(name, transition, None, None)
    }

    /// Create a segment with presentation metadata and optional Python
    /// template metadata. Template execution belongs to the Python layer.
    pub fn segment_with(
        &mut self,
        name: impl Into<String>,
        transition: Option<TransitionType>,
        notes: Option<String>,
        template: Option<String>,
    ) -> Result<SegmentHandle, SegmentError> {
        self.segment_with_background(name, transition, notes, template, None)
    }

    /// Create a segment with presentation metadata and an optional full-canvas
    /// background. A missing override uses the canvas background.
    pub fn segment_with_background(
        &mut self,
        name: impl Into<String>,
        transition: Option<TransitionType>,
        notes: Option<String>,
        template: Option<String>,
        background: Option<gaanim_renderer::background::BackgroundPaint>,
    ) -> Result<SegmentHandle, SegmentError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(SegmentError::EmptyName);
        }
        // Slides read their speaker notes from the narration script.
        self.ensure_default_script()
            .map_err(|error| SegmentError::Script(Box::new(error)))?;
        // A new segment starts where the timeline currently ends, and so does
        // the transition that enters it.
        let transition_sound = transition
            .as_ref()
            .and_then(TransitionType::sound)
            .map(|cue| self.sound_track(cue, self.current_time()))
            .transpose()
            .map_err(SegmentError::Sound)?;
        let site = super::timeline_index::authoring_site();
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let normalized_name = name.to_lowercase();
        if guard
            .segments
            .iter()
            .any(|segment| segment.explicit && segment.name.to_lowercase() == normalized_name)
        {
            return Err(SegmentError::DuplicateName { name });
        }

        let replace_implicit = guard.segments.len() == 1
            && guard.active_idx == 0
            && guard.segments[0].is_untouched_implicit();
        if replace_implicit && transition.is_some() {
            return Err(SegmentError::FirstTransition);
        }

        let id = guard.next_segment_id();
        if let Some(site) = site {
            guard.authored_timeline.segments.insert(id, site);
        }
        let mut segment = Segment::new(id, name.clone(), notes, template.clone(), background);
        if replace_implicit {
            // Markers authored at t=0 before the first segment belong to it.
            segment.markers = std::mem::take(&mut guard.segments[0].markers);
            guard.segments[0] = segment;
            guard.active_idx = 0;
        } else {
            let previous = guard.active_idx;
            segment.transition = transition;
            segment.prev_segment = Some(previous);
            let index = guard.segments.len();
            guard.segments.push(segment);
            guard.active_idx = index;
        }
        // Constraints made for one segment let go as the next starts.
        for order in std::mem::take(&mut guard.segment_camera_bindings) {
            guard.active_mut().ops.push(Op::SwitchCameraBinding {
                order,
                enabled: false,
            });
        }
        let segment_number = guard
            .segments
            .iter()
            .filter(|segment| segment.explicit)
            .count();
        drop(guard);
        self.profile.segment_started(&name);

        if !replace_implicit {
            self.set_transition_sound(id, transition_sound);
        }
        self.spawn_segment_branding(template.as_deref(), segment_number)?;
        Ok(SegmentHandle::new(id, self.state.clone()))
    }

    /// Return the camera to the scene's 2D view at the cursor, at once: every
    /// camera constraint made so far lets go and the camera resets. A
    /// segment that starts with it (`segment(..., camera="reset")`) shows its
    /// own view from its first frame; like a cut, it comes after a stop at
    /// its instant, and the segment before keeps its camera while a
    /// transition shows it leave.
    pub fn release_camera(&mut self) -> Result<(), PlayError> {
        {
            let mut state = self.state.lock().expect("canvas state poisoned");
            let made = state.next_camera_binding_order;
            let ops = &mut state.active_mut().ops;
            ops.extend((0..made).map(|order| Op::SwitchCameraBinding {
                order,
                enabled: false,
            }));
        }
        let reset = self.camera_reset(0.0);
        self.play_items_configured(vec![PlayItem::Animation(reset)], None, None)
    }

    /// Explicitly link two segments created by this canvas.
    pub fn link(
        &mut self,
        from: &SegmentHandle,
        to: &SegmentHandle,
        transition: TransitionType,
    ) -> Result<(), SegmentError> {
        if !from.belongs_to(&self.state) || !to.belongs_to(&self.state) {
            return Err(SegmentError::ForeignSegment);
        }
        let guard = self.state.lock().expect("canvas state poisoned");
        let from_index = guard
            .segments
            .iter()
            .position(|segment| segment.id == from.id())
            .ok_or(SegmentError::UnknownSegment { id: from.id() })?;
        let to_index = guard
            .segments
            .iter()
            .position(|segment| segment.id == to.id())
            .ok_or(SegmentError::UnknownSegment { id: to.id() })?;
        if from_index >= to_index {
            return Err(SegmentError::InvalidLink);
        }
        let segment_start: f64 = guard.segments[..to_index]
            .iter()
            .map(|segment| segment.cursor)
            .sum();
        let to_id = guard.segments[to_index].id;
        drop(guard);
        let sound = transition
            .sound()
            .map(|cue| self.sound_track(cue, segment_start))
            .transpose()
            .map_err(SegmentError::Sound)?;
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.segments[to_index].transition = Some(transition);
        guard.segments[to_index].prev_segment = Some(from_index);
        drop(guard);
        self.set_transition_sound(to_id, sound);
        Ok(())
    }

    // -- Object factories --

    pub fn circle(&mut self, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Circle(r))
    }
    pub fn rect(&mut self, w: f64, h: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Rect(w, h))
    }
    pub fn rounded_rect(&mut self, w: f64, h: f64, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RoundedRect(w, h, r))
    }
    pub fn surrounding_rect(
        &mut self,
        targets: Vec<BoundsTarget>,
        padding: [f64; 4],
        corner_radius: f64,
    ) -> Result<SurroundingRectHandle, SurroundingRectError> {
        surrounding_rect_in(&self.state, targets, padding, corner_radius)
    }
    pub fn square(&mut self, s: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Square(s))
    }
    pub fn dot(&mut self, r: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Dot(r))
    }
    pub fn ellipse(&mut self, rx: f64, ry: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Ellipse(rx, ry))
    }
    pub fn line(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Line(x1, y1, x2, y2))
    }

    /// Create a 2D line centered at the origin with a length in scene units.
    /// Its orientation is independent of subsequent `next_to` placement.
    /// Rejects nonpositive/nonfinite lengths and zero, nonfinite, or 3D directions.
    pub fn line_with_length(
        &mut self,
        length: f64,
        direction: gaanim_layout::Direction,
    ) -> Result<DrawableHandle, String> {
        if !length.is_finite() || length <= 0.0 {
            return Err("line length must be a finite positive number".into());
        }
        let vector = match direction {
            gaanim_layout::Direction::Custom(vector) => vector,
            direction => direction.to_vector(),
        };
        let magnitude = vector.x.hypot(vector.y);
        if !vector.is_finite() || vector.z != 0.0 || magnitude == 0.0 || !magnitude.is_finite() {
            return Err("line direction must be a finite nonzero 2D vector".into());
        }
        let half = vector / magnitude * (length * 0.5);
        Ok(self.line(-half.x, -half.y, half.x, half.y))
    }

    /// Create a vector boolean result while retaining the source drawables.
    pub fn boolean(
        &mut self,
        operands: &[&DrawableHandle],
        op: BooleanOperation,
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    ) -> Result<DrawableHandle, BooleanError> {
        if operands.len() < 2 {
            return Err(BooleanError::TooFewOperands);
        }
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return Err(BooleanError::InvalidTolerance);
        }
        if operands
            .iter()
            .any(|operand| !Arc::ptr_eq(&operand.state, &self.state))
        {
            return Err(BooleanError::ForeignScene);
        }
        if operands.iter().any(|operand| {
            let spec = operand.spec.lock().expect("object spec poisoned");
            matches!(
                spec.kind,
                SpawnKind::Line(..)
                    | SpawnKind::Arrow(..)
                    | SpawnKind::SizedArrow { .. }
                    | SpawnKind::DashedLine { .. }
                    | SpawnKind::DoubleArrow { .. }
                    | SpawnKind::Arc { .. }
                    | SpawnKind::Polyline { .. }
                    | SpawnKind::ReactivePolyline { .. }
                    | SpawnKind::Bezier { .. }
                    | SpawnKind::Curve(_)
                    | SpawnKind::Image { .. }
                    | SpawnKind::Video { .. }
                    | SpawnKind::Lottie { .. }
                    | SpawnKind::Primitive3D(_)
                    | SpawnKind::Polyline3D { .. }
                    | SpawnKind::ReactivePolyline3D { .. }
                    | SpawnKind::LineSegments3D { .. }
            )
        }) {
            return Err(BooleanError::NonVectorOperand);
        }
        let result = self.spawn(SpawnKind::Boolean {
            sources: operands.iter().map(|operand| operand.id).collect(),
            op,
            live,
            tolerance,
            rule,
        });
        // A boolean is a new drawable, but uses the first operand's visual
        // treatment so it composes naturally with existing artwork.
        let source = operands[0]
            .spec
            .lock()
            .expect("object spec poisoned")
            .clone();
        let mut target = result.spec.lock().expect("object spec poisoned");
        target.fill = source.fill;
        target.fill_overridden = source.fill_overridden;
        target.stroke = source.stroke;
        target.stroke_style = source.stroke_style;
        target.stroke_overridden = source.stroke_overridden;
        target.opacity = source.opacity;
        target.opacity_overridden = source.opacity_overridden;
        target.glow = source.glow;
        target.blur = source.blur;
        target.shadow = source.shadow;
        target.blend = source.blend;
        drop(target);
        Ok(result)
    }

    /// Live plexus lines between `points`, drawables of this scene: the
    /// members of a single group, or the drawables themselves. They follow
    /// the points as they move; `mode` picks the pairs within
    /// `max_distance` (every pair, the `neighbors` nearest of each point, or
    /// each point and the next) and `fade` draws longer links fainter.
    pub fn connect(
        &mut self,
        points: &[&DrawableHandle],
        max_distance: f64,
        mode: gaanim_renderer::effects::ConnectMode,
        neighbors: usize,
        fade: bool,
    ) -> Result<DrawableHandle, String> {
        if points
            .iter()
            .any(|point| !Arc::ptr_eq(&point.state, &self.state))
        {
            return Err("connect() points must belong to this scene".to_string());
        }
        let sources: Vec<gaanim_core::ObjectId> = match points {
            [group] => match &group.spec.lock().expect("object spec poisoned").kind {
                SpawnKind::Group(members) | SpawnKind::GroupNoCenter(members) => members.clone(),
                _ => vec![group.id],
            },
            _ => points.iter().map(|point| point.id).collect(),
        };
        if sources.len() < 2 {
            return Err("connect() needs at least two points or a group of them".to_string());
        }
        if max_distance.is_nan() || max_distance <= 0.0 {
            return Err(format!("max_distance must be positive, got {max_distance}"));
        }
        if neighbors == 0 {
            return Err("neighbors must be at least 1".to_string());
        }
        let result = self.spawn(SpawnKind::Connect {
            sources,
            max_distance,
            mode,
            neighbors,
            fade,
        });
        {
            // A thin default pen; themes restyle it like any line.
            let mut spec = result.spec.lock().expect("object spec poisoned");
            spec.stroke = Some((Brush::Solid(Color::WHITE), 0.02));
            spec.fill = None;
        }
        Ok(result)
    }

    pub fn union(
        &mut self,
        operands: &[&DrawableHandle],
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    ) -> Result<DrawableHandle, BooleanError> {
        self.boolean(operands, BooleanOperation::Union, live, tolerance, rule)
    }
    pub fn intersection(
        &mut self,
        operands: &[&DrawableHandle],
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    ) -> Result<DrawableHandle, BooleanError> {
        self.boolean(
            operands,
            BooleanOperation::Intersection,
            live,
            tolerance,
            rule,
        )
    }
    pub fn difference(
        &mut self,
        operands: &[&DrawableHandle],
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    ) -> Result<DrawableHandle, BooleanError> {
        self.boolean(
            operands,
            BooleanOperation::Difference,
            live,
            tolerance,
            rule,
        )
    }
    pub fn xor(
        &mut self,
        operands: &[&DrawableHandle],
        live: bool,
        tolerance: f64,
        rule: BooleanRule,
    ) -> Result<DrawableHandle, BooleanError> {
        self.boolean(operands, BooleanOperation::Xor, live, tolerance, rule)
    }

    /// Fill a vector mask from one edge. The mask remains an ordinary drawable,
    /// which acts as the optional outline when it has a stroke.
    pub fn fill_level(
        &mut self,
        mask: &DrawableHandle,
        paint: Brush,
        level: f64,
        direction: FillLevelDirection,
        keep_outline: bool,
    ) -> Result<DrawableHandle, BooleanError> {
        if !Arc::ptr_eq(&mask.state, &self.state) {
            return Err(BooleanError::ForeignScene);
        }
        if !level.is_finite() || !(0.0..=1.0).contains(&level) {
            return Err(BooleanError::InvalidFillLevel);
        }
        let result = self.spawn(SpawnKind::FillLevel {
            mask: mask.id,
            level,
            direction,
        });
        let result = result.fill_brush(paint);
        result
            .spec
            .lock()
            .expect("object spec poisoned")
            .fill_level_cursor = Some(level);
        if keep_outline {
            let outline = self.spawn(SpawnKind::FillLevelOutline { mask: mask.id });
            let source = mask.spec.lock().expect("object spec poisoned").clone();
            let mut outline_spec = outline.spec.lock().expect("object spec poisoned");
            outline_spec.fill = None;
            outline_spec.fill_overridden = true;
            outline_spec.stroke = source.stroke;
            outline_spec.stroke_style = source.stroke_style;
            outline_spec.stroke_overridden = source.stroke_overridden;
        }
        Ok(result)
    }

    /// Spawn a visible line between static or reactive endpoints.
    /// Line between fixed or reactive endpoints. Two fixed 2D points make an
    /// ordinary line, which moves with its group; reactive endpoints are
    /// re-resolved in world space every frame.
    pub fn line_between(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        match (&from, &to) {
            (CanvasEndpoint::Static(start), CanvasEndpoint::Static(end))
                if start.z == 0.0 && end.z == 0.0 =>
            {
                self.line(start.x, start.y, end.x, end.y)
            }
            _ => self.endpoint_line(from, to, false),
        }
    }
    /// A dashed line between two endpoints, `dash_length` long and
    /// `gap_length` apart; like [`Self::line_between`], it follows endpoints
    /// that move.
    pub fn dashed_line_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        dash_length: f64,
        gap_length: f64,
    ) -> DrawableHandle {
        match (&from, &to) {
            (CanvasEndpoint::Static(start), CanvasEndpoint::Static(end))
                if start.z == 0.0 && end.z == 0.0 =>
            {
                self.dashed_line(start.x, start.y, end.x, end.y, dash_length, gap_length)
            }
            _ => {
                let handle = self.spawn(SpawnKind::TrackingLine);
                self.state
                    .lock()
                    .expect("canvas state poisoned")
                    .active_mut()
                    .ops
                    .push(Op::AttachTrackingLine {
                        target: handle.id,
                        from,
                        to,
                        dashes: Some((dash_length, gap_length)),
                    });
                handle
            }
        }
    }
    pub fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Arrow(x1, y1, x2, y2))
    }
    /// Spawn a dimensioned arrow. Units are scene units; the optional ratio
    /// caps head length relative to shaft length, scaling head width with it.
    /// Invalid endpoints, dimensions or ratios fail before modifying the scene.
    pub fn arrow_with_dimensions(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        head_length: f64,
        head_width: f64,
        body_width: f64,
        max_head_ratio: Option<f64>,
    ) -> Result<DrawableHandle, &'static str> {
        if ![start.0, start.1, end.0, end.1]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("arrow endpoints must be finite");
        }
        let length = (end.0 - start.0).hypot(end.1 - start.1);
        let (head_length, head_width) =
            capped_arrow_head(length, head_length, head_width, body_width, max_head_ratio)?;
        Ok(self.spawn(SpawnKind::SizedArrow {
            start,
            end,
            head_length,
            head_width,
            body_width,
        }))
    }
    pub fn dashed_line(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        dash_length: f64,
        gap_length: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::DashedLine {
            start: (x1, y1),
            end: (x2, y2),
            dash_length,
            gap_length,
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub fn double_arrow(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head_length: Option<f64>,
        head_width: Option<f64>,
        body_width: Option<f64>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::DoubleArrow {
            start: (x1, y1),
            end: (x2, y2),
            head_length,
            head_width,
            body_width,
        })
    }
    pub fn polygon(&mut self, points: Vec<(f64, f64)>) -> DrawableHandle {
        self.spawn(SpawnKind::Polygon(points))
    }

    /// One drawable holding a filled circle of `radius` at each position,
    /// independent of any coordinate space. Positions are in scene units.
    /// Rejects an empty list, non-finite positions, and a non-positive or
    /// non-finite radius.
    pub fn points(
        &mut self,
        positions: Vec<(f64, f64)>,
        radius: f64,
    ) -> Result<DrawableHandle, String> {
        if positions.is_empty() {
            return Err("points requires at least one position".into());
        }
        if positions
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
        {
            return Err("points positions must be finite".into());
        }
        if !radius.is_finite() || radius <= 0.0 {
            return Err("points radius must be finite and greater than zero".into());
        }
        Ok(self.spawn(SpawnKind::Points { positions, radius }))
    }
    pub fn star(&mut self, points: u32, outer_radius: f64, inner_radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Star {
            points,
            outer_radius,
            inner_radius,
        })
    }
    pub fn regular_polygon(&mut self, sides: u32, radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RegularPolygon { sides, radius })
    }
    pub fn sector(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Sector {
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
        })
    }
    pub fn annulus(&mut self, outer_radius: f64, inner_radius: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Annulus {
            outer_radius,
            inner_radius,
        })
    }
    pub fn brace(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, height: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Brace {
            start: (x1, y1),
            end: (x2, y2),
            height,
        })
    }
    pub fn checkmark(&mut self, size: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Checkmark(size))
    }
    pub fn cross(&mut self, size: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Cross(size))
    }
    pub fn right_angle(&mut self, arm_length: f64) -> DrawableHandle {
        self.spawn(SpawnKind::RightAngle(arm_length))
    }
    /// Creates an open circular arc. Angles are expressed in radians.
    pub fn arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Arc {
            center: (cx, cy),
            radii: (radius, radius),
            start_angle,
            sweep_angle,
        })
    }
    /// Creates an open elliptical arc with radii `rx` along x and `ry`
    /// along y. Angles are expressed in radians, counter-clockwise from +x.
    pub fn ellipse_arc(
        &mut self,
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Arc {
            center: (cx, cy),
            radii: (rx, ry),
            start_angle,
            sweep_angle,
        })
    }
    /// Creates a curved arrow between two points.
    pub fn curved_arrow(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::CurvedArrow {
            start: (x1, y1),
            end: (x2, y2),
            angle,
            head_length: DEFAULT_ARROW_HEAD_LENGTH,
            head_width: DEFAULT_ARROW_HEAD_WIDTH,
            body_width: DEFAULT_ARROW_BODY_WIDTH,
        })
    }
    /// Spawn a dimensioned curved arrow between two points, with the same
    /// units and validation as [`Self::arrow_with_dimensions`]. The optional
    /// ratio caps head length relative to the arc length.
    #[allow(clippy::too_many_arguments)]
    pub fn curved_arrow_with_dimensions(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        angle: f64,
        head_length: f64,
        head_width: f64,
        body_width: f64,
        max_head_ratio: Option<f64>,
    ) -> Result<DrawableHandle, &'static str> {
        if ![start.0, start.1, end.0, end.1, angle]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("curved arrow endpoints and angle must be finite");
        }
        let length = gaanim_objects::primitives::curved_arrow_length(
            gaanim_core::kurbo::Point::new(start.0, start.1),
            gaanim_core::kurbo::Point::new(end.0, end.1),
            angle,
        );
        let (head_length, head_width) =
            capped_arrow_head(length, head_length, head_width, body_width, max_head_ratio)?;
        Ok(self.spawn(SpawnKind::CurvedArrow {
            start,
            end,
            angle,
            head_length,
            head_width,
            body_width,
        }))
    }
    /// Creates a curved arrow along an explicit circular arc. Angles are in radians.
    pub fn curved_arrow_arc(
        &mut self,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::CurvedArrowArc {
            center: (cx, cy),
            radius,
            start_angle,
            sweep_angle,
            head_length: DEFAULT_ARROW_HEAD_LENGTH,
            head_width: DEFAULT_ARROW_HEAD_WIDTH,
            body_width: DEFAULT_ARROW_BODY_WIDTH,
        })
    }
    /// Spawn a dimensioned curved arrow along an explicit circular arc, with
    /// the same units and validation as [`Self::curved_arrow_with_dimensions`].
    #[allow(clippy::too_many_arguments)]
    pub fn curved_arrow_arc_with_dimensions(
        &mut self,
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_angle: f64,
        head_length: f64,
        head_width: f64,
        body_width: f64,
        max_head_ratio: Option<f64>,
    ) -> Result<DrawableHandle, &'static str> {
        if ![center.0, center.1, radius, start_angle, sweep_angle]
            .iter()
            .all(|v| v.is_finite())
        {
            return Err("curved arrow center, radius and angles must be finite");
        }
        let (head_length, head_width) = capped_arrow_head(
            radius.abs() * sweep_angle.abs(),
            head_length,
            head_width,
            body_width,
            max_head_ratio,
        )?;
        Ok(self.spawn(SpawnKind::CurvedArrowArc {
            center,
            radius,
            start_angle,
            sweep_angle,
            head_length,
            head_width,
            body_width,
        }))
    }
    /// Creates a dimension line offset perpendicularly from the measured segment.
    pub fn dimension(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, offset: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Dimension {
            start: (x1, y1),
            end: (x2, y2),
            offset,
        })
    }
    /// Creates an open path connecting the given points in order.
    ///
    /// Use this for technical geometry such as springs, rails, or trajectories.
    pub fn polyline(&mut self, points: &[(f64, f64)]) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline {
            points: points.to_vec(),
            closed: false,
        })
    }

    /// Creates a path connecting the given points in order and the last one
    /// to the first. Unlike [`Self::polygon`], it has no fill of its own.
    pub fn closed_polyline(&mut self, points: &[(f64, f64)]) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline {
            points: points.to_vec(),
            closed: true,
        })
    }

    /// The outline of `balls` melted together like drops of liquid: each
    /// is taken as a circle (its center and half its larger side) and the
    /// outline follows them every frame as they move and scale. Balls
    /// closer than `smoothness` join with a neck (0 is a plain union);
    /// `threshold` divides every radius. The balls stop being drawn.
    pub fn metaballs(
        &mut self,
        balls: &[&DrawableHandle],
        threshold: f64,
        smoothness: f64,
    ) -> Result<DrawableHandle, String> {
        if balls.iter().any(|ball| !self.owns_drawable(ball)) {
            return Err("a metaball belongs to another scene".to_string());
        }
        if !(threshold.is_finite() && threshold > 0.0) {
            return Err("threshold must be a positive number".to_string());
        }
        if !(smoothness.is_finite() && smoothness >= 0.0) {
            return Err("smoothness must be zero or a positive number".to_string());
        }
        let sources = balls.iter().map(|ball| ball.id).collect();
        for ball in balls {
            let _ = (*ball).clone().opacity(0.0);
        }
        Ok(self.spawn(SpawnKind::Metaballs {
            sources,
            threshold,
            smoothness,
        }))
    }

    /// A polyline whose coordinates are reactive sources, redrawn every
    /// frame from their values; `closed` joins the last point to the first.
    pub fn reactive_polyline(
        &mut self,
        points: Vec<(
            gaanim_animation::ScalarSource,
            gaanim_animation::ScalarSource,
        )>,
        closed: bool,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::ReactivePolyline { points, closed })
    }

    /// Create a native quadratic or cubic Bézier path.
    pub fn bezier(
        &mut self,
        start: (f64, f64),
        controls: Vec<(f64, f64)>,
        end: (f64, f64),
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Bezier {
            start,
            controls,
            end,
        })
    }

    /// Create a composed native curve from cursor commands.
    ///
    /// Commands may move the cursor, add line or Bézier segments, and close a
    /// subpath. Points on a relative command are offsets from the current
    /// cursor. `CurveControl::Auto` mirrors the preceding matching control
    /// point; `CurveControl::None` collapses that handle onto the endpoint.
    pub fn curve(&mut self, elements: Vec<crate::canvas::CurveElement>) -> DrawableHandle {
        self.spawn(SpawnKind::Curve(elements))
    }
    /// Creates Cartesian axes — manim `Axes` compatible.
    ///
    /// Mirrors `manim.mobject.graphing.coordinate_systems.Axes`:
    /// `x_range`/`y_range` as `(min, max, step)`, `x_length`/`y_length` control
    /// scene size (like Manim), `tips` adds arrowheads, `axis_config` dicts are
    /// accepted for compatibility (mapped to `axis_color`/`include_numbers` etc).
    /// `auto_fit=true` (default) scales data to `safe_frame` (gaanim layout idiom);
    /// set `auto_fit=false` or explicit `x_length`/`y_length` for Manim-like fixed size.
    ///
    /// The returned `DrawableHandle` is animable: `axes.create()` draws
    /// sequentially `Grid → Axes → Ticks → Numbers/Labels` via `PathCompletion`
    /// leaves (`crates/gaanim_api/src/builder.rs` expansion). Use
    /// `scene.plot(axes, f, x_range)` or `axes.coords_to_point(x,y)` /
    /// `axes.point_to_coords(p)` (manim `coords_to_point`/`point_to_coords`),
    /// `scene.plot_parametric_curve(axes, f, t_range)`, and
    /// `axes.get_x_axis()`/`get_y_axis()` for compatibility.
    pub fn axes(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        config: crate::canvas::AxesConfig,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Axes {
            x_range,
            y_range,
            config,
        })
    }

    /// Creates a triangulated 3D surface mesh in world space.
    /// Vertices are expected in world coordinates.
    pub fn surface_mesh(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        color: Option<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::SurfaceMesh {
            vertices,
            indices,
            color,
            colors: None,
        })
    }

    pub fn cube(
        &mut self,
        size: f64,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cube(size, material)?)))
    }

    pub fn sphere(
        &mut self,
        radius: f64,
        segments: u32,
        rings: u32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::sphere(
            radius, segments, rings, material,
        )?)))
    }

    pub fn cylinder(
        &mut self,
        radius: f64,
        height: f64,
        segments: u32,
        caps: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let mesh = primitives3d::cylinder(radius, height, segments, caps, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(self.along_world_up(mesh))))
    }

    pub fn cone(
        &mut self,
        radius: f64,
        height: f64,
        segments: u32,
        cap: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let mesh = primitives3d::cone(radius, height, segments, cap, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(self.along_world_up(mesh))))
    }

    pub fn plane(
        &mut self,
        width: f64,
        height: f64,
        subdivisions: (u32, u32),
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let mesh = primitives3d::plane(width, height, subdivisions, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(self.along_world_up(mesh))))
    }

    /// `mesh`, built with its axis along y, turned to stand along the
    /// world's up: a quarter turn about x takes y to z when z is up.
    fn along_world_up(
        &self,
        mut mesh: gaanim_scene::TriangleMeshData,
    ) -> gaanim_scene::TriangleMeshData {
        if self.world_up != DVec3::Z {
            return mesh;
        }
        let turn = |[x, y, z]: [f32; 3]| [x, -z, y];
        for vertex in &mut mesh.vertices {
            *vertex = turn(*vertex);
        }
        for normal in mesh.normals.iter_mut().flatten() {
            *normal = turn(*normal);
        }
        mesh
    }

    /// A torus about the world's up axis: a tube `minor_radius` thick
    /// around a circle of `major_radius` in the XZ plane (XY when z is up).
    pub fn torus(
        &mut self,
        major_radius: f64,
        minor_radius: f64,
        segments: u32,
        rings: u32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let mesh = primitives3d::torus(major_radius, minor_radius, segments, rings, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(self.along_world_up(mesh))))
    }

    /// A closed, lit arrow from `start` to `end` in world space, placed at
    /// its midpoint so it moves, turns and grows about it.
    pub fn arrow_3d(
        &mut self,
        start: [f64; 3],
        end: [f64; 3],
        sizes: primitives3d::ArrowSizes,
        segments: u32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let arrow = primitives3d::arrow_3d(
            DVec3::from_array(start),
            DVec3::from_array(end),
            sizes,
            segments,
            material,
        )?;
        Ok(self.spawn_placed_mesh(arrow))
    }

    /// A lit tube `radius` thick along the polyline through `points` in
    /// world space, without twisting; `closed` makes it a loop and `caps`
    /// closes the ends of an open one. Placed at the center of the points'
    /// box.
    pub fn tube(
        &mut self,
        points: &[[f64; 3]],
        radius: f64,
        segments: u32,
        closed: bool,
        caps: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let points: Vec<DVec3> = points.iter().copied().map(DVec3::from_array).collect();
        let tube = primitives3d::tube(&points, radius, segments, closed, caps, material)?;
        Ok(self.spawn_placed_mesh(tube))
    }

    /// A lit 3D line `thickness` across between two endpoints, which follows
    /// them as they move: a cylinder of `segments` sides, capped unless
    /// `caps` is false, that every frame lays between them by its transform,
    /// so its mesh stays the same. `create` grows it from `from` and
    /// `uncreate` takes it back there.
    pub fn line_3d(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        thickness: f64,
        segments: u32,
        caps: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, Line3DError> {
        {
            let state = self.state.lock().expect("canvas state poisoned");
            if !(endpoint_is_valid(&from, &state) && endpoint_is_valid(&to, &state)) {
                return Err(Line3DError::InvalidEndpoint);
            }
        }
        let mesh = primitives3d::cylinder(thickness / 2.0, 1.0, segments, caps, material)?;
        let handle = self.spawn(SpawnKind::Primitive3D(mesh));
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingLine3D {
                target: handle.id,
                from,
                to,
            });
        Ok(handle)
    }

    /// A lit mesh of `vertices` in its own coordinates (its origin stays the
    /// drawable's) and `triangles` indexing them, with one optional color per
    /// vertex.
    pub fn mesh_3d(
        &mut self,
        vertices: &[[f64; 3]],
        triangles: &[[u32; 3]],
        colors: Option<Vec<Color>>,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let vertices: Vec<DVec3> = vertices.iter().copied().map(DVec3::from_array).collect();
        let colors = colors.map(|colors| {
            colors
                .iter()
                .map(|color| {
                    let rgba = color.to_rgba8();
                    [rgba.r, rgba.g, rgba.b, rgba.a].map(|channel| f32::from(channel) / 255.0)
                })
                .collect()
        });
        let mesh = primitives3d::custom_mesh(&vertices, triangles, colors, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(mesh)))
    }

    /// The surface where `field` equals `level`, a lit mesh in `material`
    /// whose origin is the field's.
    pub fn isosurface(
        &mut self,
        field: &gaanim_objects::isosurface::SampledField,
        level: f32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, gaanim_objects::isosurface::IsosurfaceError> {
        let surface = gaanim_objects::isosurface::isosurface(field, level)?;
        Ok(self.spawn(SpawnKind::Primitive3D(surface.mesh(material))))
    }

    /// `field` drawn as a cloud that glows and absorbs light (see
    /// [`gaanim_scene::VolumeData`]): each value, placed in `value_range`
    /// (by default the field's finite values' span), takes the sRGB color
    /// and opacity `transfer` holds for it, and a value of full opacity
    /// absorbs `density` of the light behind it per scene unit. A value that
    /// is not finite is empty. The drawable's origin is the field's.
    pub fn volume(
        &mut self,
        field: &gaanim_objects::isosurface::SampledField,
        transfer: &[[u8; 4]; 256],
        value_range: Option<(f32, f32)>,
        density: f32,
    ) -> DrawableHandle {
        let finite = || {
            field
                .values
                .iter()
                .copied()
                .filter(|value| value.is_finite())
        };
        let (low, high) = value_range.unwrap_or_else(|| {
            (
                finite().reduce(f32::min).unwrap_or(0.0),
                finite().reduce(f32::max).unwrap_or(1.0),
            )
        });
        // A range from high to low reverses the values.
        let span = if (high - low).abs() < f32::EPSILON {
            f32::EPSILON
        } else {
            high - low
        };
        let texels: Vec<u8> = field
            .values
            .iter()
            .map(|value| {
                if value.is_finite() {
                    ((value - low) / span).clamp(0.0, 1.0).mul_add(255.0, 0.5) as u8
                } else {
                    0
                }
            })
            .collect();
        let transfer_bytes: Vec<u8> = transfer.iter().flatten().copied().collect();
        self.spawn(SpawnKind::Volume(gaanim_scene::VolumeData {
            values: gaanim_scene::VolumeData::pack(&texels),
            size: field.size.map(|side| side as u32),
            min: field.min.as_vec3().to_array(),
            max: field.max.as_vec3().to_array(),
            transfer: gaanim_core::peniko::ImageData {
                data: gaanim_core::peniko::Blob::new(std::sync::Arc::new(transfer_bytes)),
                format: gaanim_core::peniko::ImageFormat::Rgba8,
                alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
                width: 256,
                height: 1,
            },
            density: density.max(0.0),
        }))
    }

    /// One drawable that draws `prototype`'s mesh, with its material, at
    /// every one of `copies`: the mesh uploads once and every copy draws in
    /// one instanced draw. The prototype stays a template, hidden unless
    /// `keep_prototype`.
    pub fn instances(
        &mut self,
        prototype: &DrawableHandle,
        copies: gaanim_scene::MeshCopies,
        keep_prototype: bool,
    ) -> Result<DrawableHandle, crate::mesh_copies::InstancesError> {
        if !std::sync::Arc::ptr_eq(&prototype.state, &self.state) {
            return Err(crate::mesh_copies::InstancesError::ForeignPrototype);
        }
        let mesh = {
            let spec = prototype.spec.lock().expect("object spec poisoned");
            match &spec.kind {
                SpawnKind::Primitive3D(mesh)
                    if spec.extrusion.is_none() && spec.copies.is_none() =>
                {
                    mesh.clone()
                }
                _ => return Err(crate::mesh_copies::InstancesError::NotAPrimitive),
            }
        };
        // A 3D line's mesh is a cylinder its endpoints lay into place.
        let tracks_endpoints = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .flat_map(|segment| &segment.ops)
            .any(|op| matches!(op, Op::AttachTrackingLine3D { target, .. } if *target == prototype.id));
        if tracks_endpoints {
            return Err(crate::mesh_copies::InstancesError::NotAPrimitive);
        }
        let handle = self.copies_of(mesh, copies)?;
        if !keep_prototype {
            prototype.clone().opacity(0.0);
        }
        Ok(handle)
    }

    /// One drawable with a lit sphere of `radius` at each of `positions`,
    /// colored by `colors` when given (see [`Self::instances`]).
    pub fn points_3d(
        &mut self,
        positions: &[[f64; 3]],
        radius: f64,
        (segments, rings): (u32, u32),
        colors: Option<Vec<Color>>,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, crate::mesh_copies::InstancesError> {
        let mesh = primitives3d::sphere(radius, segments, rings, material)?;
        let copies = gaanim_scene::MeshCopies {
            positions: positions
                .iter()
                .map(|point| point.map(|value| value as f32))
                .collect(),
            scales: None,
            rotations: None,
            colors: colors
                .map(|colors| colors.into_iter().map(crate::mesh_copies::linear).collect()),
        };
        self.copies_of(mesh, copies)
    }

    fn copies_of(
        &mut self,
        mesh: gaanim_scene::TriangleMeshData,
        copies: gaanim_scene::MeshCopies,
    ) -> Result<DrawableHandle, crate::mesh_copies::InstancesError> {
        if copies.positions.is_empty() {
            return Err(crate::mesh_copies::InstancesError::NoPositions);
        }
        if !copies.is_consistent() {
            return Err(crate::mesh_copies::InstancesError::LengthMismatch);
        }
        let finite = copies.positions.iter().flatten().all(|v| v.is_finite())
            && copies
                .scales
                .iter()
                .flatten()
                .flatten()
                .all(|v| v.is_finite())
            && copies
                .rotations
                .iter()
                .flatten()
                .all(|q| q.iter().all(|v| v.is_finite()) && q.iter().any(|v| *v != 0.0))
            && copies
                .colors
                .iter()
                .flatten()
                .flatten()
                .all(|v| v.is_finite());
        if !finite {
            return Err(crate::mesh_copies::InstancesError::NotFinite);
        }
        let handle = self.spawn(SpawnKind::Primitive3D(mesh));
        handle.spec.lock().expect("object spec poisoned").copies =
            Some(std::sync::Arc::new(copies));
        Ok(handle)
    }

    /// A lit solid swept by a `(r, y)` profile around the y axis, its
    /// origin the profile's.
    pub fn revolve(
        &mut self,
        profile: &[[f64; 2]],
        segments: u32,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        let profile: Vec<gaanim_core::glam::DVec2> = profile
            .iter()
            .copied()
            .map(gaanim_core::glam::DVec2::from_array)
            .collect();
        let mesh = primitives3d::revolve(&profile, segments, material)?;
        Ok(self.spawn(SpawnKind::Primitive3D(mesh)))
    }

    fn spawn_placed_mesh(&mut self, placed: primitives3d::PlacedMesh) -> DrawableHandle {
        let center = placed.center;
        self.spawn(SpawnKind::Primitive3D(placed.mesh))
            .move_to_3d(center.x, center.y, center.z)
    }

    /// A light at the origin for lit 3D content: move it like any drawable.
    /// While the scene shows lights they replace the fixed key light.
    pub fn light_3d(
        &mut self,
        kind: gaanim_scene::LightKind3D,
        color: Color,
        intensity: f32,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Light3D(gaanim_scene::Light3D {
            kind,
            color,
            intensity,
        }))
    }

    pub fn lighting_3d(&mut self, enabled: bool, intensity: f32, shadows: bool) {
        self.lighting_3d = gaanim_scene::Lighting3D {
            enabled,
            intensity,
            shadows,
            z_up: false,
        };
    }

    /// How translucent 3D content blends: each drawable as one layer, from
    /// back to front (`layers == 0`), or every surface in its depth order at
    /// each pixel, up to `layers` deep (see [`gaanim_scene::Transparency3D`]).
    pub fn transparency_3d(&mut self, layers: u8) {
        self.transparency_3d = gaanim_scene::Transparency3D {
            layers: layers.min(gaanim_scene::Transparency3D::MAX_LAYERS),
        };
    }

    pub fn surface_mesh_with_colors(
        &mut self,
        vertices: Vec<[f32; 3]>,
        indices: Vec<u32>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::SurfaceMesh {
            vertices,
            indices,
            color: None,
            colors: Some(colors),
        })
    }

    /// Creates a 3D polyline from world-space points.
    pub fn polyline_3d(&mut self, points: Vec<[f32; 3]>) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline3D {
            points,
            colors: None,
        })
    }

    /// Creates a 3D polyline with per-vertex colors (e.g. colormap).
    /// `colors` must have the same length as `points`; otherwise a uniform color fallback is used.
    pub fn polyline_3d_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Polyline3D {
            points,
            colors: Some(colors),
        })
    }

    /// A 3D polyline whose coordinates are reactive sources, redrawn every
    /// frame from their values, with one color per point when `colors`
    /// matches `points` in length.
    pub fn reactive_polyline_3d(
        &mut self,
        points: Vec<[gaanim_animation::ScalarSource; 3]>,
        colors: Option<Vec<Color>>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::ReactivePolyline3D { points, colors })
    }

    pub(crate) fn line_segments_3d_with_colors(
        &mut self,
        points: Vec<[f32; 3]>,
        colors: Vec<Color>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::LineSegments3D {
            points,
            colors: Some(colors),
        })
    }
    pub fn text(&mut self, s: &str) -> DrawableHandle {
        let spec = gaanim_text::prelude::TextSpec::new(
            vec![s.into()],
            None,
            gaanim_text::prelude::TextStyle::default(),
            gaanim_text::prelude::TextFlow::default(),
        )
        .expect("SceneModel::text received invalid text; public bindings validate input first");
        self.text_spec(spec)
    }

    /// Creates unified structured text. This is the owning Rust entry point
    /// used by the Python `Scene.text(*content, ...)` factory.
    pub fn text_spec(&mut self, spec: gaanim_text::prelude::TextSpec) -> DrawableHandle {
        let mut handle = self.spawn(SpawnKind::Text(spec.clone()));
        for part in spec.parts() {
            if let Some(color) = part.style.color {
                handle = handle.color_nth(part.text.clone(), part.occurrence, color);
            }
            handle = handle.define_tag(part.path.join("."), part.text, Some(part.occurrence));
        }
        handle
    }
    /// Compile full Typst markup, including tables and other document layout.
    pub fn typst(&mut self, source: &str) -> DrawableHandle {
        self.typst_inner(source, None, None)
    }

    /// Compile full Typst markup with a custom page width (e.g. `"16cm"`, `"800pt"`, `"12in"`).
    pub fn typst_with_width(&mut self, source: &str, page_width: &str) -> DrawableHandle {
        self.typst_inner(source, Some(page_width), None)
    }

    /// Compile full Typst markup loaded from an asset path.
    ///
    /// Relative paths use the directory configured with [`Self::set_asset_root`].
    pub fn typst_asset(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<DrawableHandle, TypstAssetError> {
        self.typst_asset_inner(path, None)
    }

    /// Compile Typst markup loaded from an asset path with a custom page width.
    pub fn typst_asset_with_width(
        &mut self,
        path: impl AsRef<Path>,
        page_width: &str,
    ) -> Result<DrawableHandle, TypstAssetError> {
        self.typst_asset_inner(path, Some(page_width))
    }

    fn typst_asset_inner(
        &mut self,
        path: impl AsRef<Path>,
        page_width: Option<&str>,
    ) -> Result<DrawableHandle, TypstAssetError> {
        let path = self.resolve_asset_path(path);
        let source = std::fs::read_to_string(&path).map_err(|source| TypstAssetError::Read {
            path: path.clone(),
            source,
        })?;
        // Its own folder comes first, so it imports the files beside it.
        let folder = path
            .parent()
            .map(|folder| std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf()));
        Ok(self.typst_inner(&source, page_width, folder))
    }

    /// Where a Typst document's project files are read from (`#import
    /// "lib.typ"`, `image("logo.svg")`), first to last: the scene's asset
    /// root, or without one its search folders, then the workspace's shared
    /// asset folders.
    fn typst_roots(&self) -> Vec<PathBuf> {
        let own = match &self.asset_root {
            Some(root) => vec![root.clone()],
            None => self.asset_search.clone(),
        };
        own.into_iter()
            .chain(self.shared_asset_roots.iter().cloned())
            .filter(|folder| folder.is_dir())
            .collect()
    }

    /// Read a tmTheme file for a code block, resolved like other assets.
    pub fn read_code_theme(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<gaanim_text::prelude::TmTheme, CodeThemeError> {
        let path = self.resolve_asset_path(path);
        let xml = std::fs::read_to_string(&path).map_err(|source| CodeThemeError::Read {
            path: path.clone(),
            source,
        })?;
        gaanim_text::prelude::TmTheme::parse(&xml)
            .map_err(|message| CodeThemeError::Invalid { path, message })
    }

    fn typst_inner(
        &mut self,
        source: &str,
        page_width: Option<&str>,
        folder: Option<PathBuf>,
    ) -> DrawableHandle {
        let roots: Vec<PathBuf> = folder.into_iter().chain(self.typst_roots()).collect();
        let files = gaanim_text::prelude::read_typst_files(source, &roots);
        self.spawn(SpawnKind::Typst {
            source: source.to_string(),
            page_width: page_width.map(|w| w.to_string()),
            scene_units: false,
            files,
        })
    }

    /// Compile Typst markup whose point lengths are scene units. Built-in
    /// components use this to size generated markup directly.
    #[doc(hidden)]
    pub fn typst_in_scene_units(&mut self, source: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Typst {
            source: source.to_string(),
            page_width: None,
            scene_units: true,
            files: Default::default(),
        })
    }

    fn text_semantic_pairs(
        source: &DrawableHandle,
        target: &DrawableHandle,
        requested: Option<Vec<(String, String)>>,
    ) -> Vec<(String, Option<usize>, String, Option<usize>)> {
        let source_tags = source
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let target_tags = target
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .clone();
        let requested = requested.unwrap_or_else(|| {
            source_tags
                .iter()
                .filter_map(|(name, _, _)| {
                    target_tags
                        .iter()
                        .any(|(target_name, _, _)| target_name == name)
                        .then_some((name.clone(), name.clone()))
                })
                .collect()
        });
        requested
            .into_iter()
            .filter_map(|(source_name, target_name)| {
                let (_, source_fragment, source_occurrence) = source_tags
                    .iter()
                    .rev()
                    .find(|(tag, _, _)| tag == &source_name)?;
                let (_, target_fragment, target_occurrence) = target_tags
                    .iter()
                    .rev()
                    .find(|(tag, _, _)| tag == &target_name)?;
                Some((
                    source_fragment.clone(),
                    *source_occurrence,
                    target_fragment.clone(),
                    *target_occurrence,
                ))
            })
            .collect()
    }

    /// Auto-match and morph submobjects — improved `TransformMatchingShapes`.
    ///
    /// Matches sub-elements between `source` and `target` using Hungarian minimum-cost pairing,
    /// normalized shape hashing, relative world position, and color similarity.
    /// Matched source sub-elements morph into target sub-elements in exact world space,
    /// surplus source elements fade out, and new target elements fade in.
    pub fn transform_matching_shapes(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        duration: f64,
    ) -> DrawableHandle {
        self.queue_transform_matching(source, target, false, duration)
    }

    /// Generic auto-matching morph. `mode` is `"shapes"`, or `"tex"` (also
    /// `"text"`) to pair the glyphs of two texts or equations by character.
    ///
    /// Queues an `Op::TransformMatching` operation to pair and morph submobjects between
    /// `source` and `target`. At the end the target is shown and the source
    /// hidden. Another `mode` is an error.
    pub fn transform_matching(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        mode: &str,
        duration: f64,
    ) -> Result<DrawableHandle, super::MatchingModeError> {
        let by_glyph = match mode.to_ascii_lowercase().as_str() {
            "shapes" => false,
            "tex" | "text" => true,
            _ => {
                return Err(super::MatchingModeError {
                    mode: mode.to_string(),
                });
            }
        };
        Ok(self.queue_transform_matching(source, target, by_glyph, duration))
    }

    fn queue_transform_matching(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        by_glyph: bool,
        duration: f64,
    ) -> DrawableHandle {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return target.clone();
        }
        let mode = if by_glyph { "tex" } else { "shapes" }.to_string();
        let semantic_pairs = if by_glyph {
            Self::text_semantic_pairs(source, target, None)
        } else {
            Vec::new()
        };
        // Played like any other animation: it starts at the cursor, freezes
        // declaration state, and advances the cursor by its duration.
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.freeze_spawn_specs();
        let segment = guard.active_mut();
        segment.cursor += duration;
        segment.ops.push(Op::TransformMatching {
            source: source.id,
            target: target.id,
            mode,
            semantic_pairs,
            duration,
        });
        target.clone()
    }

    /// Load a PNG, JPEG, or WebP image as an animatable raster mobject.
    ///
    /// Source pixels are decoded once per canonical path. With no explicit
    /// destination size, the image is contained inside the logical safe frame.
    pub fn image(&mut self, path: impl AsRef<Path>) -> Result<super::ImageHandle, ImageLoadError> {
        self.image_with_options(path, ImageOptions::default())
    }

    /// `options` sized to the safe area when they give no size: the source
    /// fits there, while its frame stays the fitted size (see
    /// `ImageOptions::media_frame`).
    fn sized_to_safe_frame(&self, options: ImageOptions) -> ImageOptions {
        if options.width.is_some() || options.height.is_some() {
            return options;
        }
        let safe = self.safe_frame();
        ImageOptions {
            width: Some(safe.width()),
            height: Some(safe.height()),
            ..options
        }
    }

    /// Load an image with an optional target size, fit mode, and source crop.
    pub fn image_with_options(
        &mut self,
        path: impl AsRef<Path>,
        options: ImageOptions,
    ) -> Result<super::ImageHandle, ImageLoadError> {
        let image = load_image(self.resolve_asset_path(path))?;
        let view = self
            .sized_to_safe_frame(options)
            .resolve(image.width, image.height)?;
        let drawable = self.spawn(SpawnKind::Image { image, view });
        drawable
            .spec
            .lock()
            .expect("object spec poisoned")
            .media_frame = Some(options.media_frame(view));
        Ok(drawable)
    }

    /// Load a timeline-synchronized MP4 as an animatable raster drawable.
    pub fn video(&mut self, path: impl AsRef<Path>) -> Result<VideoClip, VideoLoadError> {
        self.video_with_options(path, VideoOptions::default())
    }

    /// Load a video with temporal, sizing, loop, and embedded-audio options.
    ///
    /// Constructing the drawable does not advance or schedule it. Playback is
    /// activated by [`Self::play_items`]; a non-looping video then freezes on
    /// its last selected frame.
    pub fn video_with_options(
        &mut self,
        path: impl AsRef<Path>,
        options: VideoOptions,
    ) -> Result<VideoClip, VideoLoadError> {
        Self::check_video_options(&options)?;
        let path = self.resolve_asset_path(path);
        let metadata = gaanim_media::probe_video(&path)?;
        self.spawn_video(path, metadata, None, options)
    }

    /// Load the frames of an animated GIF, WebP or PNG (APNG) as a silent
    /// video: it plays, loops, changes speed and takes segments like one,
    /// without FFmpeg. A still image is one frame.
    pub fn animated_image_with_options(
        &mut self,
        path: impl AsRef<Path>,
        options: VideoOptions,
    ) -> Result<VideoClip, VideoLoadError> {
        Self::check_video_options(&options)?;
        let path = self.resolve_asset_path(path);
        let frames = gaanim_media::frames::VideoFrames::decode(&path)?;
        let metadata = frames.metadata();
        let options = VideoOptions {
            audio: false,
            ..options
        };
        self.spawn_video(path, metadata, Some(Arc::new(frames)), options)
    }

    /// Show image files one after another, `fps` per second, as a silent
    /// video like [`Self::animated_image_with_options`]. Each file is read
    /// when its frame is shown; every one must have the first one's size.
    pub fn image_sequence_with_options(
        &mut self,
        paths: &[PathBuf],
        fps: f64,
        options: VideoOptions,
    ) -> Result<VideoClip, VideoLoadError> {
        Self::check_video_options(&options)?;
        let paths: Vec<PathBuf> = paths
            .iter()
            .map(|path| self.resolve_asset_path(path))
            .collect();
        let frames = gaanim_media::frames::VideoFrames::sequence(paths.clone(), fps)?;
        let metadata = frames.metadata();
        let options = VideoOptions {
            audio: false,
            ..options
        };
        self.spawn_video(paths[0].clone(), metadata, Some(Arc::new(frames)), options)
    }

    /// The files a numbered `pattern` such as `render/frame_%04d.png` names,
    /// by increasing number (see [`gaanim_media::frames::numbered_files`]);
    /// its folder is found like other assets.
    pub fn numbered_files(
        &self,
        pattern: impl AsRef<Path>,
    ) -> Result<Vec<PathBuf>, VideoLoadError> {
        let pattern = pattern.as_ref();
        let folder = self.resolve_asset_path(pattern.parent().unwrap_or(Path::new("")));
        let name = pattern.file_name().unwrap_or_default();
        Ok(gaanim_media::frames::numbered_files(&folder.join(name))?)
    }

    fn check_video_options(options: &VideoOptions) -> Result<(), VideoLoadError> {
        for (name, value, positive) in [
            ("offset", options.offset, false),
            ("speed", options.speed, true),
            ("volume", options.volume, false),
        ] {
            if !value.is_finite() || if positive { value <= 0.0 } else { value < 0.0 } {
                return Err(VideoLoadError::InvalidNumber {
                    name,
                    requirement: if positive { "positive" } else { "non-negative" },
                });
            }
        }
        if let Some(duration) = options.duration
            && (!duration.is_finite() || duration <= 0.0)
        {
            return Err(VideoLoadError::InvalidNumber {
                name: "duration",
                requirement: "positive",
            });
        }
        Ok(())
    }

    /// A video drawable of `path`, or of `frames` when given, described by
    /// `metadata`.
    fn spawn_video(
        &mut self,
        path: PathBuf,
        metadata: gaanim_media::VideoMetadata,
        frames: Option<Arc<gaanim_media::frames::VideoFrames>>,
        options: VideoOptions,
    ) -> Result<VideoClip, VideoLoadError> {
        if options.offset >= metadata.duration {
            return Err(VideoLoadError::OffsetOutOfRange);
        }
        let source_duration = options
            .duration
            .unwrap_or(metadata.duration - options.offset);
        if options.offset + source_duration > metadata.duration + 1e-6 {
            return Err(VideoLoadError::DurationOutOfRange);
        }
        let view = self
            .sized_to_safe_frame(options.image)
            .resolve(metadata.width, metadata.height)?;
        let poster = match &frames {
            Some(frames) => frames.picture(frames.index_at(options.offset))?,
            None => gaanim_media::decode_video_frame(&path, &metadata, options.offset)?,
        };
        let has_audio = metadata.has_audio;
        let playback = gaanim_media::VideoPlayback {
            path: path.canonicalize().unwrap_or(path),
            metadata,
            scene_start: 0.0,
            source_offset: options.offset,
            source_duration,
            looping: options.looping,
            speed: options.speed,
            audio: options.audio,
            volume: options.volume,
            last_frame: None,
            active: false,
            intervals: Vec::new(),
            frames,
        };
        let audio = if options.audio && has_audio {
            Some(AudioTrack::from_media(
                playback.path.clone(),
                0.0,
                playback.source_offset,
                playback.source_duration,
                playback.speed,
                playback.looping,
                playback.volume,
            )?)
        } else {
            None
        };
        let duration = (!playback.looping).then_some(playback.source_duration / playback.speed);
        let drawable = self.spawn(SpawnKind::Video {
            poster,
            view,
            playback,
        });
        drawable
            .spec
            .lock()
            .expect("object spec poisoned")
            .media_frame = Some(options.image.media_frame(view));
        Ok(VideoClip {
            drawable,
            state: self.state.clone(),
            duration,
            audio,
            activated: Arc::new(AtomicBool::new(false)),
        })
    }

    /// Load a Lottie JSON composition as a timeline-synchronized vector drawable.
    pub fn lottie(&mut self, path: impl AsRef<Path>) -> Result<LottieClip, LottieLoadError> {
        self.lottie_with_options(path, LottieOptions::default())
    }

    /// Load a Lottie JSON composition with playback and destination sizing options.
    pub fn lottie_with_options(
        &mut self,
        path: impl AsRef<Path>,
        options: LottieOptions,
    ) -> Result<LottieClip, LottieLoadError> {
        self.lottie_with_package_options(path, options, Default::default())
    }
    /// Load JSON or dotLottie with explicit package selectors.
    pub fn lottie_with_package_options(
        &mut self,
        path: impl AsRef<Path>,
        mut options: LottieOptions,
        selectors: gaanim_renderer::lottie::LottiePackageOptions,
    ) -> Result<LottieClip, LottieLoadError> {
        use gaanim_renderer::lottie::{LottieAsset, LottieError, PackagePlayback};
        let path = self.resolve_asset_path(path);
        let mut package = if path
            .extension()
            .is_some_and(|v| v.eq_ignore_ascii_case("lottie"))
        {
            Some(PackagePlayback::load(&path, &selectors)?)
        } else {
            if selectors != Default::default() {
                return Err(LottieError::Package(
                    "package selectors require a .lottie file".into(),
                )
                .into());
            }
            None
        };
        if let Some(package) = &mut package {
            package.fit = match options.fit {
                ImageFit::Contain => "contain",
                ImageFit::Cover => "cover",
                ImageFit::Stretch => "stretch",
            };
            if package.is_machine()
                && (options.offset != 0.0
                    || options.duration.is_some()
                    || options.looping
                    || options.speed != 1.0)
            {
                return Err(LottieError::Package(
                    "state machines control their own offset, duration, loop and speed".into(),
                )
                .into());
            }
        }
        let asset = if let Some(package) = &package {
            package.initial_asset()?
        } else {
            LottieAsset::load(&path)?
        };
        let width =
            u32::try_from(asset.width()).map_err(|_| LottieLoadError::DimensionsOutOfRange)?;
        let height =
            u32::try_from(asset.height()).map_err(|_| LottieLoadError::DimensionsOutOfRange)?;
        if options.width.is_none() && options.height.is_none() {
            let safe = self.safe_frame();
            options.width = Some(safe.width());
            options.height = Some(safe.height());
        }
        let view = ImageOptions {
            width: options.width,
            height: options.height,
            fit: options.fit,
            crop: None,
            quality: Default::default(),
        }
        .resolve(width, height)?;
        let mut playback = gaanim_renderer::lottie::LottiePlayback::new(
            asset.clone(),
            view,
            options.offset,
            options.duration,
            options.looping,
            options.speed,
        )?;
        let machine = package.as_ref().is_some_and(PackagePlayback::is_machine);
        playback.package = package;
        let duration =
            (!machine && !playback.looping).then_some(playback.source_duration / playback.speed);
        let drawable = self.spawn(SpawnKind::Lottie { playback });
        Ok(LottieClip {
            drawable,
            state: self.state.clone(),
            duration,
            activated: Arc::new(AtomicBool::new(false)),
            asset,
        })
    }

    /// Import scene `selector` of the glTF 2.0 model at `path` (`.gltf` or
    /// `.glb`): a group per node in its rest pose, holding a lit mesh per
    /// primitive, under a root at the model's origin. The root's parts are
    /// its nodes by name, and its animations play with
    /// [`DrawableHandle::animation`].
    pub fn gltf(
        &mut self,
        path: impl AsRef<Path>,
        selector: &gaanim_objects::gltf::GltfSceneSelector,
    ) -> Result<DrawableHandle, gaanim_objects::gltf::GltfLoadError> {
        let model = gaanim_objects::gltf::GltfModel::load(self.resolve_asset_path(path), selector)?;
        let count = model.nodes.len();
        let skin_data: Vec<(Arc<[usize]>, Arc<[gaanim_core::glam::DMat4]>)> = model
            .skins
            .iter()
            .map(|skin| {
                (
                    Arc::from(skin.joints.as_slice()),
                    Arc::from(skin.inverse_bind.as_slice()),
                )
            })
            .collect();
        let mut handles: Vec<Option<DrawableHandle>> = vec![None; count];
        let mut node_ids = vec![None; count];
        let mut parts = HashMap::new();
        let mut skins = Vec::new();
        let mut morphs = Vec::new();
        let mut morph_targets = Vec::new();
        // Children come after their parents: they are built first.
        for index in (0..count).rev() {
            let node = &model.nodes[index];
            let mut members = Vec::new();
            let mut morphed = Vec::new();
            let weights: Arc<[f32]> = node.weights.iter().map(|weight| *weight as f32).collect();
            for primitive in &node.primitives {
                let handle =
                    self.spawn_registered(SpawnKind::Primitive3D(primitive.mesh.clone()), false);
                let surface = primitive.mesh.surface.as_ref();
                let skinned = surface.is_some_and(|surface| surface.skin.is_some());
                if skinned && let Some((joints, bind)) = node.skin.map(|skin| &skin_data[skin]) {
                    skins.push((handle.id, Arc::clone(joints), Arc::clone(bind)));
                }
                if surface.is_some_and(|surface| !surface.morph_targets.is_empty()) {
                    handle
                        .spec
                        .lock()
                        .expect("glTF primitive spec poisoned")
                        .morph_weights = Some(Arc::clone(&weights));
                    morphs.push((handle.id, index));
                    morphed.push(handle.clone());
                }
                members.push(handle);
            }
            members.extend(
                node.children
                    .iter()
                    .filter_map(|child| handles[*child].take()),
            );
            let handle = self
                .spawn_registered(
                    SpawnKind::GroupNoCenter(members.iter().map(|member| member.id).collect()),
                    false,
                )
                .with_style_targets(
                    members
                        .iter()
                        .flat_map(|member| member.inherited_style_targets())
                        .collect(),
                );
            handle
                .spec
                .lock()
                .expect("glTF node spec poisoned")
                .gltf_pose = Some(node.rest);
            let handle = if morphed.is_empty() {
                handle
            } else {
                let targets = Arc::new(super::drawable::GltfMorphTargets {
                    names: node.morph_targets.iter().cloned().collect(),
                    primitives: morphed,
                });
                morph_targets.push(Arc::clone(&targets));
                handle.with_morph_targets(vec![targets])
            };
            parts.insert(node.name.clone(), handle.clone());
            node_ids[index] = Some(handle.id);
            handles[index] = Some(handle);
        }
        let roots: Vec<DrawableHandle> = model
            .roots
            .iter()
            .filter_map(|root| handles[*root].take())
            .collect();
        let root = self
            .spawn(SpawnKind::GroupNoCenter(
                roots.iter().map(|root| root.id).collect(),
            ))
            .with_style_targets(
                roots
                    .iter()
                    .flat_map(|root| root.inherited_style_targets())
                    .collect(),
            );
        root.spec.lock().expect("glTF model spec poisoned").gltf_rig =
            Some(crate::gltf::GltfRigSpec {
                nodes: node_ids.into_iter().flatten().collect(),
                skins,
                morphs,
            });
        // The model's nodes, as the file lists them.
        morph_targets.reverse();
        Ok(root
            .with_gltf(parts, Arc::clone(&model.animations))
            .with_morph_targets(morph_targets))
    }

    /// Load an SVG as an animatable group of resolved vector paths.
    ///
    /// Shapes, paths, solid or gradient paints, outlined text, transforms,
    /// CSS, `<use>`, `viewBox`, and vector `clipPath` groups are imported.
    /// Raster images, patterns, masks, and arbitrary filters remain omitted.
    ///
    /// The document is centered on the origin at
    /// [`SVG_PIXELS_PER_UNIT`](gaanim_objects::prelude::SVG_PIXELS_PER_UNIT)
    /// SVG pixels per logical unit, the factor used by the rest of the engine,
    /// so a 200×100 px document spans 2×1 units and a 3 px stroke is 0.03.
    pub fn svg(&mut self, path: impl AsRef<Path>) -> Result<DrawableHandle, SvgLoadError> {
        let document = gaanim_objects::prelude::SvgDocument::load(self.resolve_asset_path(path))?;
        Ok(self.svg_document(&document))
    }

    fn svg_document(&mut self, document: &gaanim_objects::prelude::SvgDocument) -> DrawableHandle {
        let mut parts = HashMap::new();
        let (root, _) = self.spawn_svg_group(&document.root, true, &mut parts);
        if !document.root.id.is_empty() {
            parts.insert(document.root.id.clone(), root.clone());
        }
        root.with_svg_parts(parts)
    }

    fn spawn_svg_group(
        &mut self,
        group: &gaanim_objects::prelude::SvgGroup,
        register_top_level: bool,
        parts: &mut HashMap<String, DrawableHandle>,
    ) -> (DrawableHandle, Vec<crate::canvas::ops::SharedObjectSpec>) {
        let mut children = Vec::new();
        let mut leaf_specs = Vec::new();
        for node in &group.children {
            match node {
                gaanim_objects::prelude::SvgNode::Path(path) => {
                    let handle = self.spawn_registered(
                        SpawnKind::SvgPath(Box::new(path.as_ref().clone())),
                        false,
                    );
                    leaf_specs.push(handle.spec.clone());
                    {
                        let mut spec = handle.spec.lock().expect("SVG path spec poisoned");
                        // SVG centers a stroke on its path, closed or not.
                        spec.stroke_align = Some(gaanim_renderer::effects::StrokeAlign::Center);
                    }
                    if !path.id.is_empty() {
                        handle.spec.lock().expect("SVG path spec poisoned").svg_id =
                            Some(path.id.clone());
                        parts.insert(path.id.clone(), handle.clone());
                    }
                    children.push(handle);
                }
                gaanim_objects::prelude::SvgNode::Group(child_group) => {
                    let (handle, descendants) = self.spawn_svg_group(child_group, false, parts);
                    leaf_specs.extend(descendants);
                    children.push(handle);
                }
            }
        }

        // Keep the invisible clip geometry inside the clipped group's hierarchy.
        // Group-level layout operations (scale, rotation, translation) must affect
        // the mask exactly like the visible descendants they clip.
        let clip_mask = group.clip_path.as_ref().map(|clip_path| {
            let rect = clip_path.bounding_box();
            self.spawn_registered(
                SpawnKind::SvgPath(Box::new(gaanim_objects::prelude::SvgPath {
                    id: String::new(),
                    path: clip_path.clone(),
                    bounds: gaanim_math::Bounds3D::new_2d(rect.x0, rect.y0, rect.x1, rect.y1),
                    fill: None,
                    stroke: gaanim_scene::StrokeBrush::transparent(),
                })),
                false,
            )
        });
        if let Some(mask) = &clip_mask {
            mask.spec
                .lock()
                .expect("SVG clip mask spec poisoned")
                .exclude_from_parent_draw = true;
            children.push(mask.clone());
        }

        let child_ids = children.iter().map(|child| child.id).collect();
        let mut handle = self.spawn_registered(SpawnKind::Group(child_ids), register_top_level);
        handle.spec.lock().expect("SVG group spec poisoned").opacity = group.opacity;
        handle = handle.with_style_targets(leaf_specs.clone());
        if let Some(sigma) = group.blur_sigma {
            handle = handle.blur(sigma);
        }
        if let Some(shadow) = &group.shadow {
            handle = handle.shadow(
                shadow.color,
                DVec2::new(shadow.offset_x, shadow.offset_y),
                shadow.blur_radius,
            );
        }
        if let Some(mask) = clip_mask {
            handle = handle.clip(&mask, gaanim_core::peniko::Fill::NonZero);
        }
        if !group.id.is_empty() {
            handle.spec.lock().expect("SVG group spec poisoned").svg_id = Some(group.id.clone());
            parts.insert(group.id.clone(), handle.clone());
        }
        (handle, leaf_specs)
    }

    pub fn group(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        self.spawn(SpawnKind::Group(members.iter().map(|m| m.id).collect()))
            .with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            )
    }

    /// A group whose origin is `origin`, in scene coordinates, instead of
    /// the center of its members' box: it turns, scales and skews about that
    /// point, and `move_to` places it, as the pivot of a rigid body.
    pub fn group_about(&mut self, members: &[&DrawableHandle], origin: DVec3) -> DrawableHandle {
        let group = self.group(members);
        group.spec.lock().expect("group spec poisoned").group_origin = Some(origin);
        group
    }

    /// Build the stable, equation-style row used by reactive numeric readouts.
    #[doc(hidden)]
    pub fn reactive_readout_group(
        &mut self,
        label: Option<&DrawableHandle>,
        equals: Option<&DrawableHandle>,
        number: &DrawableHandle,
        unit: Option<&DrawableHandle>,
        spacing: f64,
    ) -> DrawableHandle {
        let mut members = Vec::with_capacity(4);
        if let Some(label) = label {
            members.push(label);
        }
        if let Some(equals) = equals {
            members.push(equals);
        }
        members.push(number);
        if let Some(unit) = unit {
            members.push(unit);
        }
        let group = self.group(&members);
        group
            .spec
            .lock()
            .expect("readout group spec poisoned")
            .reactive_readout_layout = Some(ReactiveReadoutLayoutSpec {
            label: label.map(|part| part.id),
            equals: equals.map(|part| part.id),
            number: number.id,
            unit: unit.map(|part| part.id),
            spacing,
            align: 0.0,
        });
        group
    }

    pub(crate) fn group_no_center(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        let defer_visibility = members.iter().any(|member| {
            member
                .spec
                .lock()
                .expect("object spec poisoned")
                .defer_visibility_until_play
        });
        let handle = self
            .spawn(SpawnKind::GroupNoCenter(
                members.iter().map(|m| m.id).collect(),
            ))
            .with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            );
        if defer_visibility {
            handle.defer_visibility_until_play();
        }
        handle
    }

    /// Updates the direct children of a group created by [`Self::group`].
    /// This is used by the persistent layout container; regular group users
    /// can continue treating groups as immutable.
    pub fn set_group_members(&mut self, group: &DrawableHandle, members: &[&DrawableHandle]) {
        let mut spec = group.spec.lock().expect("group spec poisoned");
        let background = spec.layout_background;
        if let SpawnKind::Group(children) = &mut spec.kind {
            *children = background
                .into_iter()
                .chain(members.iter().map(|member| member.id))
                .collect();
        }
    }

    /// Decorate a layout group with a background excluded from content measurement.
    /// Call before its first reflow; the returned child can be styled independently.
    pub fn decorate_layout(
        &mut self,
        container: &DrawableHandle,
        fill: Option<Brush>,
        border: Option<(Brush, f64)>,
        radius: f64,
    ) -> Result<DrawableHandle, &'static str> {
        if !Arc::ptr_eq(&container.state, &self.state) {
            return Err("card must belong to this Scene");
        }
        if !radius.is_finite()
            || radius < 0.0
            || border
                .as_ref()
                .is_some_and(|(_, width)| !width.is_finite() || *width < 0.0)
        {
            return Err("card radius and border width must be finite and nonnegative");
        }
        {
            let spec = container.spec.lock().expect("object spec poisoned");
            if !matches!(spec.kind, SpawnKind::Group(_)) || spec.layout_background.is_some() {
                return Err("card needs an undecorated layout group");
            }
        }
        let mut background = self.rect(1.0, 1.0).no_fill().no_stroke().z_index(-1);
        if let Some(fill) = fill {
            background = background.fill_brush(fill);
        }
        if let Some((paint, width)) = border {
            background = background.stroke_with_style(paint, Stroke::new(width));
        }
        background
            .claim_layout(container)
            .map_err(|_| "card owns its background placement")?;
        {
            let mut spec = container.spec.lock().expect("object spec poisoned");
            spec.layout_background = Some(background.id);
            if let SpawnKind::Group(children) = &mut spec.kind {
                children.insert(0, background.id);
            }
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachLayoutBackground {
                target: background.id,
                container: container.id,
                radius,
            });
        Ok(background)
    }

    /// Show named zones in the editor's layout inspector from the current
    /// cursor to the end of the segment. Zones only compute rectangles, so
    /// this is their only trace in the compiled scene.
    pub fn record_layout_zones(&mut self, zones: Vec<(String, gaanim_math::Bounds3D)>) {
        if zones.is_empty() {
            return;
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::RecordLayoutZones { zones });
    }

    /// Record a box's new snapshot without resolving it: a transition of the
    /// box that encloses it follows at the same instant and resolves both.
    pub fn record_layout(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
    ) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let snapshot = LayoutTreeSnapshot {
            version,
            container: container.id,
            members,
            spec,
        };
        state.latest_layouts.insert(container.id, snapshot.clone());
        state.active_mut().ops.push(Op::LayoutTransition {
            from_version: version.checked_sub(1).filter(|version| *version > 0),
            to: snapshot,
            duration: None,
            entering: None,
            leaving: None,
            fold: false,
            resolve: false,
            remeasure: None,
        });
    }

    /// Queue a layout recalculation. `duration = Some(_)` animates the move
    /// and fades the newly inserted member in; `None` updates immediately.
    #[allow(clippy::too_many_arguments)]
    pub fn reflow_layout(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
        duration: Option<f64>,
        entering: Option<&DrawableHandle>,
        leaving: Option<&DrawableHandle>,
    ) {
        self.push_reflow(
            container, members, spec, version, duration, entering, leaving, None, false,
        );
    }

    /// [`Self::reflow_layout`] where `entering` unfolds into its slot and
    /// `leaving` folds out of it instead of fading: each moves with the
    /// sibling before it while the wipe [`Self::fold_layout_child`] launched
    /// opens or closes it.
    #[allow(clippy::too_many_arguments)]
    pub fn reflow_layout_folding(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
        duration: Option<f64>,
        entering: Option<&DrawableHandle>,
        leaving: Option<&DrawableHandle>,
    ) {
        self.push_reflow(
            container, members, spec, version, duration, entering, leaving, None, true,
        );
    }

    /// Clip `child` behind an edge that travels in `direction`, the main
    /// axis of its box, and open it (`opening`) or close it over `duration`
    /// seconds with the smooth easing layout transitions use, without moving
    /// the cursor. A child keeps one such clip all along its timeline, given
    /// by its first fold, which replaces any clip it had: hidden before an
    /// opening, whole before a closing. Later folds move its edge, and
    /// [`Self::unfolded_layout_child`] makes a closed child whole again.
    pub fn fold_layout_child(
        &mut self,
        child: &DrawableHandle,
        direction: super::Direction,
        opening: bool,
        duration: f64,
    ) -> Result<(), super::InvalidWipe> {
        let existing = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .fold_wipes
            .get(&child.id)
            .cloned();
        let progress = match existing {
            Some((progress, closed)) => {
                // A child whole since its last fold opens from closed.
                if opening && !closed {
                    progress.set(0.0).expect("0 is a valid parameter value");
                }
                progress
            }
            None => {
                let progress = super::visualization::parameter_in(
                    &self.state,
                    if opening { 0.0 } else { 1.0 },
                )
                .expect("a parameter starting at 0 or 1 is valid");
                super::drawable::wipe_mask(
                    &self.state,
                    child.id,
                    progress.source(),
                    direction,
                    !opening,
                )?;
                progress
            }
        };
        self.state
            .lock()
            .expect("canvas state poisoned")
            .fold_wipes
            .insert(child.id, (progress.clone(), !opening));
        let edge = progress
            .animate()
            .set(if opening { 1.0 } else { 0.0 })
            .duration(duration)
            .rate_func(RateFunc::Smooth);
        self.launch_composition_configured(Composition::leaf(edge), None, None)
            .expect("a scene parameter's animations belong to this scene");
        Ok(())
    }

    /// Make `child` whole again if it folded out of a box, so a fade-in or an
    /// instant add shows all of it.
    pub fn unfolded_layout_child(&mut self, child: &DrawableHandle) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let Some((progress, closed)) = state.fold_wipes.get_mut(&child.id) else {
            return;
        };
        if std::mem::take(closed) {
            let progress = progress.clone();
            drop(state);
            progress.set(1.0).expect("1 is a valid parameter value");
        }
    }

    /// [`Self::reflow_layout`] measuring the children of `within` (the
    /// container or a box nested in it), at any depth, with the scale and
    /// rotation they have at the cursor rather than the ones the layout first
    /// placed them with: a child scaled or rotated since then gets room for
    /// its new size.
    pub fn remeasure_layout(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
        duration: Option<f64>,
        within: &DrawableHandle,
    ) {
        self.push_reflow(
            container,
            members,
            spec,
            version,
            duration,
            None,
            None,
            Some(within.id),
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn push_reflow(
        &mut self,
        container: &DrawableHandle,
        members: Vec<LayoutMemberSpec>,
        spec: LayoutSpec,
        version: u64,
        duration: Option<f64>,
        entering: Option<&DrawableHandle>,
        leaving: Option<&DrawableHandle>,
        remeasure: Option<gaanim_core::ObjectId>,
        fold: bool,
    ) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let snapshot = LayoutTreeSnapshot {
            version,
            container: container.id,
            members,
            spec,
        };
        state.latest_layouts.insert(container.id, snapshot.clone());
        state.active_mut().ops.push(Op::LayoutTransition {
            from_version: version.checked_sub(1).filter(|version| *version > 0),
            to: snapshot,
            duration: duration.filter(|value| value.is_finite() && *value > 0.0),
            entering: entering.map(|member| member.id),
            leaving: leaving.map(|member| member.id),
            fold,
            resolve: true,
            remeasure,
        });
        if !state.layout_constraints.is_empty() {
            let constraints = state.layout_constraints.clone();
            state.active_mut().ops.push(Op::LayoutConstraints {
                constraints,
                duration: duration.filter(|value| value.is_finite() && *value > 0.0),
            });
        }
    }

    /// Register relational Layout v2 constraints and resolve them at the
    /// current timeline cursor. Registered relations are automatically
    /// replayed after later structural reflows.
    pub fn constrain_layout(
        &mut self,
        constraints: Vec<gaanim_layout::LayoutConstraint>,
        duration: Option<f64>,
    ) -> Result<(), SceneObjectError> {
        let duration = duration.filter(|value| value.is_finite() && *value > 0.0);
        let mut state = self.state.lock().expect("canvas state poisoned");
        let owns_every_reference = constraints.iter().all(|constraint| {
            constraint
                .lhs
                .terms
                .keys()
                .chain(constraint.rhs.terms.keys())
                .all(|variable| {
                    state
                        .all_drawables
                        .contains(&gaanim_core::ObjectId::from_raw(variable.node.0))
                })
        });
        if !owns_every_reference {
            return Err(SceneObjectError::ForeignScene);
        }
        let mut combined = state.layout_constraints.clone();
        combined.extend(constraints);
        let referenced: std::collections::BTreeSet<_> = combined
            .iter()
            .flat_map(|constraint| {
                constraint
                    .lhs
                    .terms
                    .keys()
                    .chain(constraint.rhs.terms.keys())
                    .map(|variable| variable.node)
            })
            .collect();
        let mut preflight = gaanim_layout::ResolvedLayout::default();
        for node in referenced {
            preflight.boxes.insert(
                node,
                gaanim_layout::ResolvedBox {
                    bounds: gaanim_math::Bounds3D::new_2d(0.0, 0.0, 1.0, 1.0),
                    clip: None,
                    scale: gaanim_core::glam::DVec3::ONE,
                },
            );
        }
        gaanim_layout::solve_constraints(&mut preflight, &combined)?;
        state.layout_diagnostics = preflight
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    None,
                    format!(
                        "constraint #{}: {} (residual {:.6})",
                        diagnostic.constraint, diagnostic.message, diagnostic.residual
                    ),
                )
            })
            .collect();
        state.layout_constraints = combined;
        let constraints = state.layout_constraints.clone();
        state.active_mut().ops.push(Op::LayoutConstraints {
            constraints,
            duration,
        });
        Ok(())
    }

    /// Diagnostics produced by the most recent layout compilation.
    pub fn check_layout(&self) -> Vec<String> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .iter()
            .map(|(_, message)| message.clone())
            .collect()
    }

    /// Diagnostics associated with one layout root.
    pub fn layout_diagnostics(&self, root: &DrawableHandle) -> Vec<String> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .layout_diagnostics
            .iter()
            .filter(|(owner, _)| owner.is_none_or(|owner| owner == root.id))
            .map(|(_, message)| message.clone())
            .collect()
    }

    /// A camera animation lasting `duration`, unless `Scene.play` gives a
    /// default duration and the animation sets none of its own.
    fn camera_anim(&self, ty: AnimationType, duration: f64) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(
            gaanim_core::ObjectId::from_raw(u64::MAX),
            ty,
            self.state.clone(),
            active_idx,
        )
        .default_duration(duration)
    }

    fn camera_state_handle(
        &self,
        source: gaanim_animation::CameraStateSource,
    ) -> CameraStateHandle {
        CameraStateHandle {
            source,
            state: self.state.clone(),
        }
    }

    /// Create a reusable concrete orthographic camera state.
    pub fn camera_state_2d(
        &self,
        center: DVec2,
        zoom: f64,
        rotation: f64,
    ) -> Result<CameraStateHandle, gaanim_math::CameraValidationError> {
        let pose = gaanim_math::CameraPose::orthographic_2d(center, zoom, rotation)?;
        Ok(self.camera_state_handle(gaanim_animation::CameraStateSource::Concrete(pose)))
    }

    /// Create a reusable concrete perspective look-at camera state.
    pub fn camera_state_3d(
        &self,
        eye: DVec3,
        target: DVec3,
        up: DVec3,
        fov_y: f64,
        near: f64,
        far: f64,
    ) -> Result<CameraStateHandle, gaanim_math::CameraValidationError> {
        self.camera_state_3d_shifted(eye, target, up, fov_y, near, far, DVec2::ZERO)
    }

    /// [`Self::camera_state_3d`] that draws `target` at `lens_shift`, in
    /// scene units of the frame from its middle.
    #[allow(clippy::too_many_arguments)]
    pub fn camera_state_3d_shifted(
        &self,
        eye: DVec3,
        target: DVec3,
        up: DVec3,
        fov_y: f64,
        near: f64,
        far: f64,
        lens_shift: DVec2,
    ) -> Result<CameraStateHandle, gaanim_math::CameraValidationError> {
        let pose = gaanim_math::CameraPose {
            lens_shift,
            ..gaanim_math::CameraPose::perspective_3d(eye, target, up, fov_y, near, far)?
        };
        pose.validate()?;
        Ok(self.camera_state_handle(gaanim_animation::CameraStateSource::Concrete(pose)))
    }

    /// A concrete perspective state that looks at `target` from `azimuth`
    /// and `elevation` radians around `up` (see
    /// [`gaanim_math::orbit_direction`]), from as far as `reach` says.
    #[allow(clippy::too_many_arguments)]
    pub fn camera_state_orbit(
        &self,
        target: DVec3,
        azimuth: f64,
        elevation: f64,
        reach: OrbitReach,
        up: DVec3,
        fov_y: f64,
        near: f64,
        far: f64,
        lens_shift: DVec2,
    ) -> Result<CameraStateHandle, gaanim_math::CameraValidationError> {
        let distance = match reach {
            OrbitReach::Distance(distance) => distance,
            OrbitReach::Scale(scale) => {
                gaanim_math::orbit_distance(scale, fov_y, self.frame.height)
            }
        };
        let pose = gaanim_math::CameraPose {
            lens_shift,
            ..gaanim_math::CameraPose::orbit_3d(
                target, azimuth, elevation, distance, up, fov_y, near, far,
            )?
        };
        pose.validate()?;
        Ok(self.camera_state_handle(gaanim_animation::CameraStateSource::Concrete(pose)))
    }

    /// Capture the authored camera at the current timeline cursor.
    pub fn camera_capture(&self) -> CameraStateHandle {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let id = state.next_camera_state_id();
        state.active_mut().ops.push(Op::CaptureCameraState { id });
        drop(state);
        self.camera_state_handle(gaanim_animation::CameraStateSource::Captured(id))
    }

    /// Save (or replace) a named camera capture at the current cursor.
    pub fn camera_save(&self, name: &str) -> Result<CameraStateHandle, CameraStateError> {
        if name.trim().is_empty() {
            return Err(CameraStateError::EmptyName);
        }
        let state_handle = self.camera_capture();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .saved_camera_states
            .insert(name.to_owned(), state_handle.source);
        Ok(state_handle)
    }

    /// Animate to a reusable concrete or captured camera state.
    /// Where a camera in `state` draws the world `point` on the frame, in
    /// the units a HUD drawable is placed in: scene units of the unmoved
    /// camera, from the center of the frame, y up. Only a state authored
    /// with [`Self::camera_state_2d`] or [`Self::camera_state_3d`] is known
    /// while the scene is written.
    pub fn camera_project(
        &self,
        state_handle: &CameraStateHandle,
        point: DVec3,
    ) -> Result<gaanim_core::glam::DVec2, CameraStateError> {
        if !Arc::ptr_eq(&self.state, &state_handle.state) {
            return Err(CameraStateError::ForeignScene);
        }
        let gaanim_animation::CameraStateSource::Concrete(pose) = state_handle.source else {
            return Err(CameraStateError::NotAuthored);
        };
        let (width, height) = self.frame.preview_pixel_size();
        let mut camera =
            gaanim_math::Camera::ortho_2d_frame(self.frame.width, self.frame.height, width, height);
        camera
            .apply_pose(pose)
            .expect("an authored camera state is a valid pose");
        Ok(camera.hud_point(point))
    }

    /// The point where the main camera draws `target` on the frame, every
    /// frame, in the units a HUD drawable is placed in: a HUD drawable that
    /// follows it stays over `target` however the camera moves.
    pub fn camera_screen_point(&self, target: CanvasEndpoint) -> PointRef {
        PointRef(CanvasEndpoint::Screen(Box::new(target)))
    }

    pub fn camera_to(
        &self,
        state_handle: &CameraStateHandle,
        duration: f64,
    ) -> Result<Anim, CameraStateError> {
        self.camera_to_along(state_handle, duration, false)
    }

    /// Animate to a camera state turning about the target, as
    /// [`gaanim_math::CameraPose::orbit_in_frame`] does: the eye goes around
    /// what it looks at instead of cutting straight across.
    pub fn camera_orbit_to(
        &self,
        state_handle: &CameraStateHandle,
        duration: f64,
    ) -> Result<Anim, CameraStateError> {
        self.camera_to_along(state_handle, duration, true)
    }

    fn camera_to_along(
        &self,
        state_handle: &CameraStateHandle,
        duration: f64,
        orbit: bool,
    ) -> Result<Anim, CameraStateError> {
        if !Arc::ptr_eq(&self.state, &state_handle.state) {
            return Err(CameraStateError::ForeignScene);
        }
        let from_id = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .next_camera_state_id();
        Ok(self
            .camera_anim(
                AnimationType::CameraState {
                    from: gaanim_animation::CameraStateSource::Captured(from_id),
                    to: state_handle.source,
                    orbit,
                },
                duration,
            )
            .capture_camera_before_play(from_id))
    }

    /// Animate to a previously saved named camera state.
    pub fn camera_restore(&self, name: &str, duration: f64) -> Result<Anim, CameraStateError> {
        let source = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .saved_camera_states
            .get(name)
            .copied()
            .ok_or_else(|| CameraStateError::UnknownName(name.to_owned()))?;
        self.camera_to(&self.camera_state_handle(source), duration)
    }

    /// Pan the orthographic camera to a world-space point.
    pub fn camera_pan_to(&mut self, x: f64, y: f64, duration: f64) -> Anim {
        let to = gaanim_core::glam::DVec3::new(x, y, self.camera_position.z);
        self.camera_anim(AnimationType::CameraPosition { to }, duration)
    }

    /// Pan toward any native reactive endpoint.
    pub fn camera_pan_to_endpoint(&mut self, target: CanvasEndpoint, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraPositionSource { target }, duration)
    }

    /// Animate orthographic zoom with perceptually uniform (exponential)
    /// interpolation. Values above one zoom in.
    pub fn camera_zoom_to(&mut self, zoom: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraZoom {
                to: zoom,
                interpolation: gaanim_math::ZoomInterpolation::Exponential,
            },
            duration,
        )
    }

    /// Animate orthographic zoom toward a native scalar source, exponentially.
    pub fn camera_zoom_to_source(&mut self, to: ScalarSource, duration: f64) -> Anim {
        self.camera_zoom_to_source_with_interpolation(
            to,
            gaanim_math::ZoomInterpolation::Exponential,
            duration,
        )
    }

    /// Animate orthographic zoom toward a native scalar source.
    pub fn camera_zoom_to_source_with_interpolation(
        &mut self,
        to: ScalarSource,
        interpolation: gaanim_math::ZoomInterpolation,
        duration: f64,
    ) -> Anim {
        self.camera_anim(
            AnimationType::CameraZoomSource { to, interpolation },
            duration,
        )
    }

    /// Pan and zoom to keep `target` inside the viewport with a uniform margin.
    pub fn camera_frame_to(&mut self, target: &DrawableHandle, margin: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraFrame {
                target: target.id,
                margin,
                interpolation: gaanim_math::ZoomInterpolation::Exponential,
            },
            duration,
        )
    }

    /// Frame one or more drawables using CSS-order margins, zooming exponentially.
    pub fn camera_frame_many(
        &mut self,
        targets: &[DrawableHandle],
        margins: [f64; 4],
        dynamic: bool,
        duration: f64,
    ) -> Anim {
        self.camera_frame_many_with_interpolation(
            targets,
            margins,
            dynamic,
            gaanim_math::ZoomInterpolation::Exponential,
            duration,
        )
    }

    /// Frame one or more drawables using CSS-order margins and an explicit
    /// zoom interpolation. Exponential mode also pans about a fixed point.
    pub fn camera_frame_many_with_interpolation(
        &mut self,
        targets: &[DrawableHandle],
        margins: [f64; 4],
        dynamic: bool,
        interpolation: gaanim_math::ZoomInterpolation,
        duration: f64,
    ) -> Anim {
        self.camera_anim(
            AnimationType::CameraFrameMany {
                targets: targets.iter().map(|target| target.id).collect(),
                margins,
                dynamic,
                interpolation,
            },
            duration,
        )
    }

    /// Rotate the 2D camera toward a reactive angle in radians.
    pub fn camera_rotate_to_source(&mut self, to: ScalarSource, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraRotationSource { to }, duration)
    }

    /// Rotate the 2D camera around the viewport center, in radians.
    pub fn camera_rotate_to(&mut self, angle: f64, duration: f64) -> Anim {
        let to = gaanim_core::glam::DQuat::from_rotation_z(angle);
        self.camera_anim(AnimationType::CameraRotation { to }, duration)
    }

    /// Keep the camera centered on `target` while its updaters run.
    pub fn camera_follow(&mut self, target: &DrawableHandle, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraFollow { target: target.id }, duration)
    }

    /// Follow any native endpoint, optionally in its local axes with deterministic lag.
    pub fn camera_follow_endpoint(
        &mut self,
        target: CanvasEndpoint,
        offset: DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
        lag: f64,
        duration: f64,
    ) -> Anim {
        self.camera_follow_endpoint_with(target, offset, offset_space, lag, false, duration)
    }

    /// [`Self::camera_follow_endpoint`]; with `turn`, a camera that looks at
    /// the scene from an angle or through a perspective stays where it is
    /// and turns to keep the endpoint at the center, as on a tripod.
    pub fn camera_follow_endpoint_with(
        &mut self,
        target: CanvasEndpoint,
        offset: DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
        lag: f64,
        turn: bool,
        duration: f64,
    ) -> Anim {
        self.camera_anim(
            AnimationType::CameraFollowEndpoint {
                target,
                offset,
                offset_space,
                lag,
                turn,
            },
            duration,
        )
    }

    /// Apply a deterministic camera shake that settles back at its start position.
    pub fn camera_shake(&mut self, amplitude: f64, frequency: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraShake {
                amplitude,
                frequency,
                trauma: None,
            },
            duration,
        )
    }

    /// Apply a trauma-driven coherent-noise shake (translation and roll).
    ///
    /// Trauma decays linearly in timeline seconds and the camera returns to
    /// rest by the end of `duration`; see [`gaanim_math::TraumaShake`].
    pub fn camera_trauma_shake(&mut self, shake: gaanim_math::TraumaShake, duration: f64) -> Anim {
        // The shake decays over its own duration.
        self.camera_anim(
            AnimationType::CameraShake {
                amplitude: shake.amplitude,
                frequency: shake.frequency,
                trauma: Some(shake),
            },
            duration,
        )
        .duration(duration)
    }

    /// Set camera to look at `target` from `eye` with `up` (3D perspective).
    pub fn camera_look_at(
        &mut self,
        eye: (f64, f64, f64),
        target: (f64, f64, f64),
        up: Option<(f64, f64, f64)>,
        duration: f64,
    ) -> Anim {
        let eye = DVec3::new(eye.0, eye.1, eye.2);
        let target = DVec3::new(target.0, target.1, target.2);
        let up = up
            .map(|(x, y, z)| DVec3::new(x, y, z))
            .unwrap_or(self.world_up);
        self.camera_anim(AnimationType::CameraLookAt { eye, target, up }, duration)
    }

    /// Aim the camera using native reactive endpoints: from `eye`, or from
    /// where the camera stands when it is `None`, toward `target`. Moving
    /// endpoints are followed while the animation plays.
    pub fn camera_look_at_endpoints(
        &mut self,
        eye: Option<CanvasEndpoint>,
        target: CanvasEndpoint,
        up: DVec3,
        duration: f64,
    ) -> Anim {
        self.camera_anim(
            AnimationType::CameraLookAtSource { eye, target, up },
            duration,
        )
    }

    /// Orbit around current target by yaw/pitch radians.
    pub fn camera_orbit(&mut self, delta_yaw: f64, delta_pitch: f64, duration: f64) -> Anim {
        self.camera_orbit_dolly(delta_yaw, delta_pitch, 1.0, duration)
    }

    /// Orbit around the current target by yaw/pitch radians while the eye's
    /// distance from it changes by `dolly` (below 1 closer), at a constant
    /// rate of change: one move on the camera pose instead of two that would
    /// both drive it.
    pub fn camera_orbit_dolly(
        &mut self,
        delta_yaw: f64,
        delta_pitch: f64,
        dolly: f64,
        duration: f64,
    ) -> Anim {
        self.camera_anim(
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
                dolly,
            },
            duration,
        )
    }

    /// Animate perspective projection parameters.
    pub fn camera_perspective(&mut self, fov_y: f64, near: f64, far: f64, duration: f64) -> Anim {
        self.camera_anim(
            AnimationType::CameraPerspective { fov_y, near, far },
            duration,
        )
    }

    /// Select orthographic projection and animate to the requested zoom.
    pub fn camera_orthographic(&mut self, zoom: f64, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraOrthographic { zoom }, duration)
    }

    /// Restore the complete authored camera rig to its default 2D pose.
    pub fn camera_reset(&mut self, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraReset, duration)
    }

    /// Dolly camera toward/away from target (factor <1 closer).
    pub fn camera_dolly(&mut self, factor: f64, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraDolly { factor }, duration)
    }

    /// Move the 2D camera along the path of `route`, read in scene
    /// coordinates when the animation is scheduled. With `follow.orient`
    /// the view turns so the direction of travel points right on screen,
    /// plus that offset (`-π/2` puts it up).
    pub fn camera_follow_path(
        &mut self,
        route: &DrawableHandle,
        follow: crate::anim::PathFollowOptions,
        duration: f64,
    ) -> Result<Anim, String> {
        if !Arc::ptr_eq(&route.state, &self.state) {
            return Err("the camera's route must belong to this scene".to_string());
        }
        if !(follow.start.is_finite() && follow.end.is_finite())
            || !(0.0..=1.0).contains(&follow.start)
            || !(0.0..=1.0).contains(&follow.end)
            || follow.start == follow.end
        {
            return Err("start and end must be different fractions in [0, 1]".to_string());
        }
        if follow.orient.is_some_and(|offset| !offset.is_finite()) {
            return Err("rotate_offset must be finite".to_string());
        }
        Ok(self.camera_anim(
            AnimationType::CameraFollowPath {
                route: route.id,
                follow,
            },
            duration,
        ))
    }

    /// A whip pan to `target`: a fast move eased hard at both ends. With
    /// `blur`, exported frames and snapshots of the move get a 360° motion
    /// blur window, whatever the scene's own motion blur.
    pub fn camera_whip_pan(&mut self, target: CanvasEndpoint, blur: bool, duration: f64) -> Anim {
        self.camera_anim(AnimationType::CameraWhipPan { target, blur }, duration)
    }

    /// Change the depth of field of parallax layers over `duration`: each
    /// layer blurs by `aperture * |1 - focus / depth|` scene units, at most
    /// `max_blur`; `None` keeps the current value (focus 1, aperture 0 and
    /// max_blur 0.5 at first, so nothing blurs).
    pub fn camera_depth_of_field(
        &mut self,
        focus: Option<f64>,
        aperture: Option<f64>,
        max_blur: Option<f64>,
        duration: f64,
    ) -> Result<Anim, String> {
        if focus.is_some_and(|focus| !focus.is_finite() || focus <= 0.0) {
            return Err("focus must be a finite positive depth".to_string());
        }
        for (name, value) in [("aperture", aperture), ("max_blur", max_blur)] {
            if value.is_some_and(|value| !value.is_finite() || value < 0.0) {
                return Err(format!("{name} must be finite and non-negative"));
            }
        }
        Ok(self.camera_anim(
            AnimationType::CameraDepthOfField {
                focus,
                aperture,
                max_blur,
            },
            duration,
        ))
    }

    /// Move where a perspective camera draws the center of its view to
    /// `offset`, in scene units of the frame from its middle: a lens shift,
    /// so its target shows there while it orbits or dollies, without turning.
    pub fn camera_screen_offset(&mut self, offset: DVec2, duration: f64) -> Result<Anim, String> {
        if !offset.is_finite() {
            return Err("screen_offset must be finite".to_string());
        }
        Ok(self.camera_anim(AnimationType::CameraLensShift { to: offset }, duration))
    }

    /// A dolly zoom (vertigo shot) to `factor` times the distance from the
    /// look-at target; needs a perspective camera aimed with `look_at`.
    pub fn camera_dolly_zoom(&mut self, factor: f64, duration: f64) -> Result<Anim, String> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err("factor must be finite and positive".to_string());
        }
        Ok(self.camera_anim(AnimationType::CameraDollyZoom { factor }, duration))
    }

    fn camera_binding(
        &mut self,
        kind: CanvasCameraBindingKind,
        influence: ScalarSource,
        enabled: bool,
    ) -> CameraConstraintHandle {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let order = state.next_camera_binding_order();
        let spec = Arc::new(Authored::sharing(
            CameraBindingSpec {
                order,
                kind,
                influence,
                enabled,
            },
            &self.state,
        ));
        state
            .active_mut()
            .ops
            .push(Op::SpawnCameraBinding(spec.clone()));
        drop(state);
        CameraConstraintHandle {
            spec,
            state: self.state.clone(),
        }
    }

    /// Bind orthographic camera channels to native reactive sources.
    pub fn camera_bind_2d(
        &mut self,
        center: Option<CanvasEndpoint>,
        zoom: Option<ScalarSource>,
        rotation: Option<ScalarSource>,
        influence: ScalarSource,
        enabled: bool,
    ) -> Result<CameraConstraintHandle, CameraBindingError> {
        if center.is_none() && zoom.is_none() && rotation.is_none() {
            return Err(CameraBindingError::Empty);
        }
        if matches!(&center, Some(CanvasEndpoint::Static(position)) if position.z.abs() > f64::EPSILON)
        {
            return Err(CameraBindingError::InvalidDimension);
        }
        if zoom
            .as_ref()
            .and_then(ScalarSource::constant_value)
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(CameraBindingError::InvalidZoom);
        }
        if influence
            .constant_value()
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(CameraBindingError::InvalidInfluence);
        }
        Ok(self.camera_binding(
            CanvasCameraBindingKind::TwoD {
                center,
                zoom,
                rotation,
            },
            influence,
            enabled,
        ))
    }

    /// Bind perspective camera channels to native reactive sources.
    #[allow(clippy::too_many_arguments)]
    pub fn camera_bind_3d(
        &mut self,
        eye: Option<CanvasEndpoint>,
        target: Option<CanvasEndpoint>,
        fov_y: Option<ScalarSource>,
        up: CanvasEndpoint,
        lens_shift: Option<[ScalarSource; 2]>,
        influence: ScalarSource,
        enabled: bool,
    ) -> Result<CameraConstraintHandle, CameraBindingError> {
        if eye.is_none() && target.is_none() && fov_y.is_none() && lens_shift.is_none() {
            return Err(CameraBindingError::Empty);
        }
        // A reactive up is checked as the frame reads it: a zero one leaves
        // the camera as it was.
        if let CanvasEndpoint::Static(up) = up
            && (!up.is_finite() || up.length_squared() <= f64::EPSILON)
        {
            return Err(CameraBindingError::InvalidUp);
        }
        if fov_y
            .as_ref()
            .and_then(ScalarSource::constant_value)
            .is_some_and(|value| {
                !value.is_finite() || !(0.0..std::f64::consts::PI).contains(&value) || value == 0.0
            })
        {
            return Err(CameraBindingError::InvalidFov);
        }
        if influence
            .constant_value()
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err(CameraBindingError::InvalidInfluence);
        }
        Ok(self.camera_binding(
            CanvasCameraBindingKind::ThreeD {
                eye,
                target,
                fov_y,
                up,
                lens_shift,
            },
            influence,
            enabled,
        ))
    }

    // -- Time controls --

    pub fn wait(&mut self, dur: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.freeze_spawn_specs();
        guard.active_mut().cursor += dur.max(0.0);
        guard.active_mut().ops.push(Op::Wait(dur.max(0.0)));
    }

    /// Set the tempo: `bpm` beats per minute, beat 0 at `offset` seconds of
    /// the timeline, in bars of `beats_per_bar` beats.
    pub fn set_tempo(&mut self, bpm: f64, offset: f64, beats_per_bar: u32) -> Result<(), String> {
        if !(bpm.is_finite() && bpm > 0.0) {
            return Err(format!("bpm must be a positive number, got {bpm}"));
        }
        if !(offset.is_finite() && offset >= 0.0) {
            return Err(format!(
                "tempo offset must be a non-negative number of seconds, got {offset}"
            ));
        }
        if beats_per_bar == 0 {
            return Err("beats_per_bar must be at least 1".to_string());
        }
        self.tempo = Some(gaanim_timeline::timeline::BeatGrid {
            bpm,
            offset,
            beats_per_bar,
        });
        Ok(())
    }

    /// The tempo set with [`Self::set_tempo`].
    pub fn tempo(&self) -> Option<gaanim_timeline::timeline::BeatGrid> {
        self.tempo
    }

    /// Seconds that `beats` beats last at the current tempo.
    pub fn beats(&self, beats: f64) -> Result<f64, String> {
        let tempo = self
            .tempo
            .ok_or("set a tempo with scene.tempo(bpm) first")?;
        if !beats.is_finite() {
            return Err(format!("beats must be finite, got {beats}"));
        }
        Ok(beats * tempo.beat())
    }

    /// Choose the instant a playback bundle shows as its cover image; `None`
    /// takes the cursor.
    pub fn set_thumbnail(&mut self, time: Option<f64>) -> Result<(), String> {
        let time = time.unwrap_or_else(|| self.current_time());
        if !time.is_finite() || time < 0.0 {
            return Err(format!(
                "thumbnail time must be a non-negative number of seconds, got {time}"
            ));
        }
        self.thumbnail_time = Some(time);
        Ok(())
    }

    /// The instant chosen with [`Self::set_thumbnail`].
    pub fn thumbnail_time(&self) -> Option<f64> {
        self.thumbnail_time
    }

    /// A warning when a launched animation still runs where the scene ends:
    /// the scene ends at the cursor, so the animation is cut there.
    pub fn launched_past_end_warning(&self) -> Option<String> {
        let end = self.current_time();
        let last = self
            .launched_channels
            .iter()
            .filter(|launched| launched.end > end + 1e-6)
            .max_by(|a, b| a.end.total_cmp(&b.end))?;
        Some(format!(
            "a launched animation ({} of its '{}' channel) runs until {:.2} s, but the scene \
             ends at {:.2} s and cuts it; add scene.wait({:.2}) at the end to show all of it",
            last.label,
            last.channel,
            last.end,
            end,
            last.end - end
        ))
    }

    /// Wait until beat `beat` (0 is the tempo's offset). A beat already
    /// behind the cursor is an error: the shot before it ran too long.
    pub fn wait_until_beat(&mut self, beat: f64) -> Result<(), String> {
        let tempo = self
            .tempo
            .ok_or("set a tempo with scene.tempo(bpm) first")?;
        if !beat.is_finite() {
            return Err(format!("beat must be finite, got {beat}"));
        }
        let target = tempo.time_of_beat(beat);
        let cursor = self.current_time();
        if target < cursor - 1e-6 {
            return Err(format!(
                "beat {beat} is at {target:.3} s, but the cursor is already at {cursor:.3} s: \
                 what comes before it is {:.3} s too long",
                cursor - target
            ));
        }
        self.wait((target - cursor).max(0.0));
        Ok(())
    }

    /// Wait until `time`, in seconds of the whole timeline. An instant
    /// already behind the cursor is an error: what comes before it is too
    /// long.
    pub fn wait_until_time(&mut self, time: f64) -> Result<(), String> {
        if !time.is_finite() {
            return Err(format!("seconds must be finite, got {time}"));
        }
        let cursor = self.current_time();
        if time < cursor - 1e-6 {
            return Err(format!(
                "{time:.3} s is behind the cursor, already at {cursor:.3} s: \
                 what comes before it is {:.3} s too long",
                cursor - time
            ));
        }
        self.wait((time - cursor).max(0.0));
        Ok(())
    }

    /// Regroup auto-queued animations into a parallel batch at the current
    /// cursor. Each `Anim` passed here is deactivated from its original
    /// sequential position before the batch is inserted.
    pub fn play(&mut self, anims: Vec<Anim>) {
        self.play_items(anims.into_iter().map(PlayItem::Animation).collect())
            .expect("invalid animation batch");
    }

    /// Activate animations and declared audio together at the current cursor.
    ///
    /// Audio with an explicit duration contributes to the play duration. An
    /// open-ended clip starts with the batch but does not extend the timeline.
    pub fn play_items(&mut self, items: Vec<PlayItem>) -> Result<(), PlayError> {
        self.play_items_configured(items, None, None)
    }

    /// Validate and atomically schedule a mixed play batch.
    pub fn play_items_configured(
        &mut self,
        items: Vec<PlayItem>,
        default_duration: Option<f64>,
        default_rate: Option<RateFunc>,
    ) -> Result<(), PlayError> {
        let children = items.into_iter().map(Composition::leaf).collect();
        let composition = Composition::parallel(children)?;
        self.play_composition_configured(composition, default_duration, default_rate)
    }

    /// Validate, resolve, and atomically schedule a composition tree.
    pub fn play_composition_configured(
        &mut self,
        composition: Composition,
        default_duration: Option<f64>,
        default_rate: Option<RateFunc>,
    ) -> Result<(), PlayError> {
        self.schedule_composition(composition, default_duration, default_rate, true)
    }

    /// Schedule a composition tree at the cursor like
    /// [`Self::play_composition_configured`], but leave the cursor where it
    /// is: the animations run while later waits, plays and cuts are
    /// scheduled over them.
    pub fn launch_composition_configured(
        &mut self,
        composition: Composition,
        default_duration: Option<f64>,
        default_rate: Option<RateFunc>,
    ) -> Result<(), PlayError> {
        self.schedule_composition(composition, default_duration, default_rate, false)
    }

    fn schedule_composition(
        &mut self,
        composition: Composition,
        default_duration: Option<f64>,
        default_rate: Option<RateFunc>,
        advance: bool,
    ) -> Result<(), PlayError> {
        let mut resolved = composition.resolved(default_duration, default_rate)?;
        resolved.sort_by(|left, right| {
            left.start
                .total_cmp(&right.start)
                .then_with(|| left.path.cmp(&right.path))
        });
        let animations = resolved
            .iter()
            .filter_map(|resolved| match &resolved.item {
                PlayItem::Animation(anim) => Some(anim),
                _ => None,
            })
            .collect::<Vec<_>>();
        if animations.iter().any(|anim| !anim.belongs_to(&self.state)) {
            return Err(PlayError::ForeignAnimation);
        }
        if animations.iter().any(|anim| anim.is_consumed()) {
            return Err(PlayError::AnimationAlreadyConsumed);
        }
        if animations
            .iter()
            .any(|anim| anim.inner.anim_type.is_empty_properties())
        {
            return Err(PlayError::EmptyAnimation);
        }
        for (index, anim) in animations.iter().enumerate() {
            if animations[..index]
                .iter()
                .any(|previous| anim.same_token(previous))
            {
                return Err(PlayError::DuplicateAnimation);
            }
        }
        for anim in &animations {
            if let crate::anim::AnimationType::CustomProperties(callback) = &anim.inner.anim_type {
                callback
                    .evaluate(anim.inner.rate_func.evaluate(0.0))
                    .map_err(PlayError::CustomAnimation)?;
                callback
                    .evaluate(anim.inner.rate_func.evaluate(1.0))
                    .map_err(PlayError::CustomAnimation)?;
            }
        }
        let mut paint_targets = std::collections::HashMap::new();
        for anim in &animations {
            anim.validate_paint_targets(&mut paint_targets)
                .map_err(|error| PlayError::InvalidPaint(error.to_owned()))?;
        }
        // Starts accumulated through sequences and staggers carry rounding
        // error, so a clip that begins where another ends may appear to
        // overlap it by a few ulps.
        const OVERLAP_EPSILON: f64 = 1e-9;
        // Property animations of one target can be merged into one.
        const PROPERTIES: &str = "Properties";
        let described =
            |label: &str, start: f64, end: f64| format!("{label} ({start:.2}–{end:.2} s)");
        // Spans are relative to the cursor; launched animations still
        // running occupy their channels too.
        let cursor = self.current_time();
        // Each occupant: target, channel, span, description, and label.
        type Occupant = (
            gaanim_core::ObjectId,
            String,
            f64,
            f64,
            String,
            &'static str,
        );
        let mut occupied: Vec<Occupant> = self
            .launched_channels
            .iter()
            .filter(|launched| launched.end > cursor + OVERLAP_EPSILON)
            // A model's new animation takes over from the one it runs.
            .filter(|launched| {
                launched.channel != GLTF_POSE || launched.start > cursor + OVERLAP_EPSILON
            })
            .map(|launched| {
                (
                    launched.target,
                    launched.channel.clone(),
                    launched.start - cursor,
                    launched.end - cursor,
                    format!(
                        "{} launched at {:.2} s (until {:.2} s)",
                        launched.label, launched.start, launched.end
                    ),
                    launched.label,
                )
            })
            .collect();
        let mut launched = Vec::new();
        for resolved_item in &resolved {
            let PlayItem::Animation(anim) = &resolved_item.item else {
                continue;
            };
            let start = resolved_item.start;
            let end = start + resolved_item.duration.unwrap_or(0.0);
            let label = crate::builder::SceneBuilder::anim_label(&anim.inner.anim_type);
            for channel in animation_channels(anim) {
                let bound = self
                    .state
                    .lock()
                    .expect("canvas state poisoned")
                    .bound_properties
                    .iter()
                    .filter(|(target, _)| *target == anim.inner.target)
                    .map(|(_, bound)| *bound)
                    .find(|bound| writes_bound_channel(anim, &channel, *bound));
                if let Some(bound) = bound {
                    return Err(PlayError::BoundProperty {
                        target: anim.inner.target,
                        channel: bound.name().to_owned(),
                    });
                }
                let conflict = occupied.iter().find(
                    |(target, occupied_channel, occupied_start, occupied_end, _, _)| {
                        if *target != anim.inner.target || *occupied_channel != channel {
                            return false;
                        }
                        let left_zero = end - start <= OVERLAP_EPSILON;
                        let right_zero = occupied_end - occupied_start <= OVERLAP_EPSILON;
                        match (left_zero, right_zero) {
                            (true, true) => (start - occupied_start).abs() <= OVERLAP_EPSILON,
                            (true, false) => {
                                start > occupied_start - OVERLAP_EPSILON
                                    && start < occupied_end - OVERLAP_EPSILON
                            }
                            (false, true) => {
                                *occupied_start > start - OVERLAP_EPSILON
                                    && *occupied_start < end - OVERLAP_EPSILON
                            }
                            (false, false) => {
                                start < occupied_end - OVERLAP_EPSILON
                                    && *occupied_start < end - OVERLAP_EPSILON
                            }
                        }
                    },
                );
                if let Some((_, _, _, _, first, first_label)) = conflict {
                    let target_kind = self
                        .state
                        .lock()
                        .expect("canvas state poisoned")
                        .object_specs
                        .get(&anim.inner.target)
                        .map_or("drawable", |spec| {
                            super::theme::spawn_name(
                                &spec.lock().expect("object spec poisoned").kind,
                            )
                        });
                    return Err(PlayError::ConflictingChannel {
                        target: anim.inner.target,
                        channel,
                        target_kind,
                        first: first.clone(),
                        second: described(label, start, end),
                        combinable: *first_label == PROPERTIES && label == PROPERTIES,
                    });
                }
                if !advance {
                    launched.push(LaunchedChannel {
                        target: anim.inner.target,
                        channel: channel.clone(),
                        start: cursor + start,
                        end: cursor + end,
                        label,
                    });
                }
                let description = described(label, start, end);
                occupied.push((anim.inner.target, channel, start, end, description, label));
            }
        }
        if resolved.iter().any(
            |item| matches!(&item.item, PlayItem::Audio(audio) if !audio.belongs_to(&self.state)),
        ) {
            return Err(PlayError::ForeignAudio);
        }
        // Anchored sounds start with their animation's resolved start, so a
        // delay, sequence position or stagger that moves the animation moves
        // the sound too.
        let mut anchored_sounds = Vec::new();
        for resolved_item in &resolved {
            let PlayItem::Animation(anim) = &resolved_item.item else {
                continue;
            };
            if let Some(cue) = anim.sound_cue() {
                let start = cursor + resolved_item.start + anim.inner.delay.max(0.0);
                anchored_sounds.push(
                    self.sound_track(cue, start)
                        .map_err(|error| PlayError::InvalidSound(error.to_string()))?,
                );
            }
        }
        if resolved.iter().any(
            |item| matches!(&item.item, PlayItem::Video(video) if !video.belongs_to(&self.state)),
        ) {
            return Err(PlayError::ForeignVideo);
        }
        if resolved
            .iter()
            .any(|item| matches!(&item.item, PlayItem::Lottie(lottie) if !lottie.belongs_to(&self.state)))
        {
            return Err(PlayError::ForeignLottie);
        }
        let mut video_activations = HashSet::new();
        if resolved.iter().any(|item| match &item.item {
            PlayItem::Video(video) => {
                video.activated.load(Ordering::Acquire)
                    || !video_activations.insert(Arc::as_ptr(&video.activated))
            }
            _ => false,
        }) {
            return Err(PlayError::VideoAlreadyActivated);
        }
        let mut pending_intervals: HashMap<
            gaanim_core::ObjectId,
            Vec<gaanim_media::VideoInterval>,
        > = HashMap::new();
        let mut segment_tokens = HashSet::new();
        for item in &resolved {
            match &item.item {
                PlayItem::Video(video) => {
                    let spec = video.drawable.spec.lock().expect("object spec poisoned");
                    let SpawnKind::Video { playback, .. } = &spec.kind else {
                        unreachable!()
                    };
                    if !playback.intervals.is_empty() || resolved.iter().any(|other| matches!(&other.item, PlayItem::VideoSegment(segment) if segment.video.drawable.id == video.drawable.id && segment.video.belongs_to(&self.state))) {
                        return Err(PlayError::MixedVideoPlayback);
                    }
                }
                PlayItem::VideoSegment(segment) => {
                    if !segment.video.belongs_to(&self.state) {
                        return Err(PlayError::ForeignVideo);
                    }
                    if segment.video.activated.load(Ordering::Acquire) {
                        return Err(PlayError::MixedVideoPlayback);
                    }
                    if segment.consumed.load(Ordering::Acquire)
                        || !segment_tokens.insert(Arc::as_ptr(&segment.consumed))
                    {
                        return Err(PlayError::VideoSegmentConsumed);
                    }
                    let mut interval = segment.interval.clone();
                    interval.scene_start = self.current_time() + item.start;
                    if !interval.scene_start.is_finite() || !interval.scene_end().is_finite() {
                        return Err(PlayError::InvalidCompositionTiming("video segment"));
                    }
                    let intervals = pending_intervals
                        .entry(segment.video.drawable.id)
                        .or_insert_with(|| {
                            let spec = segment
                                .video
                                .drawable
                                .spec
                                .lock()
                                .expect("object spec poisoned");
                            let SpawnKind::Video { playback, .. } = &spec.kind else {
                                unreachable!()
                            };
                            playback.intervals.clone()
                        });
                    if intervals.iter().any(|other| {
                        interval.scene_start < other.scene_end()
                            && other.scene_start < interval.scene_end()
                    }) {
                        return Err(PlayError::OverlappingVideoSegments);
                    }
                    intervals.push(interval);
                }
                _ => {}
            }
        }
        let mut lottie_activations = HashSet::new();
        if resolved.iter().any(|item| match &item.item {
            PlayItem::Lottie(lottie) => {
                lottie.activated.load(Ordering::Acquire)
                    || !lottie_activations.insert(Arc::as_ptr(&lottie.activated))
            }
            _ => false,
        }) {
            return Err(PlayError::LottieAlreadyActivated);
        }

        self.state
            .lock()
            .expect("canvas state poisoned")
            .freeze_spawn_specs();

        self.launched_channels
            .retain(|launched| launched.end > cursor + OVERLAP_EPSILON);
        for item in &resolved {
            let PlayItem::Animation(anim) = &item.item else {
                continue;
            };
            if matches!(anim.inner.anim_type, AnimationType::GltfAnimation { .. }) {
                let start = cursor + item.start;
                for running in &mut self.launched_channels {
                    if running.target == anim.inner.target && running.channel == GLTF_POSE {
                        running.end = running.end.min(start);
                    }
                }
            }
        }
        self.launched_channels.extend(launched);
        self.audio_tracks.extend(anchored_sounds);
        let play_start = self.current_time();
        let mut builders = Vec::new();
        let mut time_maps = Vec::new();
        let mut camera_captures = Vec::new();
        let max_duration = resolved_span(&resolved);
        let mut visual_duration: f64 = 0.0;
        for resolved_item in resolved {
            let delay = resolved_item.start;
            match resolved_item.item {
                PlayItem::Animation(anim) => {
                    anim.mark_consumed();
                    anim.commit_authoring_target();
                    if let Some(id) = anim.camera_capture_before_play() {
                        camera_captures.push(id);
                    }
                    let mut builder = anim.into_builder();
                    match resolved_item.map {
                        Some(map) => {
                            visual_duration = visual_duration
                                .max(delay.max(0.0) + resolved_item.duration.unwrap_or(0.0));
                            time_maps.push(Some(Arc::new(map)));
                        }
                        None => {
                            builder.delay += delay;
                            visual_duration = visual_duration
                                .max(builder.delay.max(0.0) + builder.duration.max(0.0));
                            time_maps.push(None);
                        }
                    }
                    builders.push(builder);
                }
                PlayItem::Audio(audio) => {
                    let mut track = audio.track;
                    track.start_time = play_start + delay;
                    audio
                        .plays
                        .lock()
                        .expect("audio plays poisoned")
                        .push(track.clone());
                    self.audio_tracks.push(track);
                }
                PlayItem::Video(video) => {
                    video.activated.store(true, Ordering::Release);
                    let start_time = play_start + delay;
                    {
                        let mut spec = video.drawable.spec.lock().expect("object spec poisoned");
                        let SpawnKind::Video { playback, .. } = &mut spec.kind else {
                            unreachable!("VideoClip must retain a video spawn kind");
                        };
                        playback.scene_start = start_time;
                        playback.active = true;
                    }
                    if let Some(mut track) = video.audio {
                        track.start_time = start_time;
                        self.audio_tracks.push(track);
                    }
                }
                PlayItem::VideoSegment(segment) => {
                    segment.consumed.store(true, Ordering::Release);
                    let mut spec = segment
                        .video
                        .drawable
                        .spec
                        .lock()
                        .expect("object spec poisoned");
                    let SpawnKind::Video { playback, .. } = &mut spec.kind else {
                        unreachable!()
                    };
                    let mut interval = segment.interval;
                    interval.scene_start = play_start + delay;
                    playback.active = true;
                    playback.intervals.push(interval);
                    playback
                        .intervals
                        .sort_by(|a, b| a.scene_start.total_cmp(&b.scene_start));
                    if let Some(mut track) = segment.audio {
                        track.start_time = play_start + delay;
                        self.audio_tracks.push(track);
                    }
                }
                PlayItem::Lottie(lottie) => {
                    lottie.activated.store(true, Ordering::Release);
                    let start_time = play_start + delay;
                    {
                        let mut spec = lottie.drawable.spec.lock().expect("object spec poisoned");
                        let SpawnKind::Lottie { playback } = &mut spec.kind else {
                            unreachable!("LottieClip must retain a Lottie spawn kind");
                        };
                        playback.scene_start = start_time;
                        playback.active = true;
                    }
                }
            }
        }

        let stack = if super::profile::tracks_script_lines() {
            super::profile::call_stack()
        } else {
            Vec::new()
        };
        let mut guard = self.state.lock().expect("canvas state poisoned");
        for id in camera_captures {
            guard.active_mut().ops.push(Op::CaptureCameraState { id });
        }
        if time_maps.iter().any(Option::is_some) {
            guard.active_mut().ops.push(Op::TimeMaps(time_maps));
        }
        if let Some(site) = stack.first() {
            let segment = guard.active();
            let play = (
                segment.id,
                segment.cursor,
                segment.cursor + max_duration,
                site.clone(),
            );
            guard.authored_timeline.plays.push(play);
        }
        guard.record_op_location(stack);
        if !advance {
            guard.active_mut().ops.push(Op::Launch(builders));
            return Ok(());
        }
        guard.active_mut().cursor += max_duration;
        guard.active_mut().ops.push(Op::Play(builders));
        let media_remainder = (max_duration - visual_duration).max(0.0);
        if media_remainder > 0.0 {
            guard.active_mut().ops.push(Op::Wait(media_remainder));
        }
        Ok(())
    }

    /// Low-level parallel playback for legacy `AnimationBuilder` values.
    pub fn play_builders(&mut self, anims: Vec<AnimationBuilder>) {
        let max_dur = anims
            .iter()
            .map(|a| a.delay.max(0.0) + a.duration.max(0.0))
            .fold(0.0, f64::max);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += max_dur;
        guard.active_mut().ops.push(Op::Play(anims));
    }

    /// Plan `target` seconds for the whole talk, or none: Presenter View
    /// counts it down while presenting.
    pub fn set_talk_target(&mut self, target: Option<f64>) -> Result<(), SegmentError> {
        self.talk_target = super::segment::segment_target(target)?;
        Ok(())
    }

    /// Configure reusable branding generated automatically for explicit segments.
    pub fn set_branding(&mut self, branding: PresentationBrand) {
        self.branding = Some(branding);
    }

    fn spawn_segment_branding(
        &mut self,
        template: Option<&str>,
        segment_number: usize,
    ) -> Result<(), SegmentError> {
        let Some(branding) = self.branding.clone() else {
            return Ok(());
        };
        // The first segment is the cover, as is any segment with a cover
        // template.
        let cover =
            segment_number == 1 || matches!(template, Some("title_slide" | "title" | "cover"));
        if cover && !branding.show_on_cover {
            return Ok(());
        }

        let frame = self.safe_frame();
        let left = frame.min.x + BRAND_INSET;
        let right = frame.max.x - BRAND_INSET;
        let palette = self.theme_style.as_ref().map(|theme| theme.palette);
        let muted = palette
            .map(|palette| palette.muted)
            .unwrap_or(Color::from_rgb8(0x94, 0xA3, 0xB8));
        let rule = palette
            .map(|palette| palette.rule)
            .unwrap_or(Color::from_rgb8(0x5B, 0x70, 0x88));
        let footer_y = frame.min.y + frame.height() * 0.018;
        let rule_y = frame.min.y + frame.height() * 0.075;

        if branding.rule {
            self.line(left, rule_y, right, rule_y)
                .no_fill()
                .stroke(rule, 0.02)
                .z_index(100);
        }
        // The number joins the footer unless it has a place of its own.
        let in_footer = branding.slide_numbers && branding.number_anchor.is_none();
        let footer = match (branding.footer.as_deref(), in_footer) {
            (Some(footer), true) => Some(format!("{footer}    ·    {segment_number:02}")),
            (Some(footer), false) => Some(footer.to_owned()),
            (None, true) => Some(format!("{segment_number:02}")),
            (None, false) => None,
        };
        if let Some(anchor) = branding.number_anchor.filter(|_| branding.slide_numbers) {
            // That point of the safe frame, inset like the footer.
            let offset = anchor.to_offset();
            let center = frame.center();
            let x = center.x + offset.x * (frame.width() * 0.5 - BRAND_INSET);
            let y = center.y + offset.y * (frame.height() * 0.5 - BRAND_INSET * 0.5);
            self.text(&format!("{segment_number:02}"))
                .fill(muted)
                .scale_to(BRAND_FOOTER_SCALE)
                .at_anchor(x, y, anchor)
                .z_index(101);
        }
        if let Some(footer) = footer {
            // Starts at the safe edge and shrinks to fit when it is too long.
            let scale = self
                .measure_text(&footer, None, None, None, None, None)
                .ok()
                .map(|(width, _)| width)
                .filter(|width| *width > f64::EPSILON)
                .map_or(BRAND_FOOTER_SCALE, |width| {
                    BRAND_FOOTER_SCALE.min((right - left) / width)
                });
            self.text(&footer)
                .fill(muted)
                .scale_to(scale)
                .at_anchor(left, footer_y + 0.08, Anchor::Left)
                .z_index(101);
        }
        if let Some(logo) = branding.logo.as_deref() {
            let extension = logo
                .extension()
                .and_then(|extension| extension.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            // Fit the logo to a fixed scene-unit height; `logo_scale`
            // multiplies it. Source pixel sizes must not leak into the scene.
            let height = BRAND_LOGO_HEIGHT * branding.logo_scale;
            let brand_error = |error: &dyn std::fmt::Display| SegmentError::BrandAsset {
                message: error.to_string(),
            };
            let logo = if extension == "svg" {
                let document =
                    gaanim_objects::prelude::SvgDocument::load(self.resolve_asset_path(logo))
                        .map_err(|error| brand_error(&error))?;
                let source_height = document
                    .root
                    .bounds()
                    .map(|bounds| bounds.height())
                    .filter(|h| h.is_finite() && *h > f64::EPSILON)
                    .unwrap_or(height);
                self.svg_document(&document)
                    .scale_to(height / source_height)
            } else {
                self.image_with_options(
                    logo,
                    ImageOptions {
                        height: Some(height),
                        ..Default::default()
                    },
                )
                .map_err(|error| brand_error(&error))?
            };
            logo.at_anchor(frame.max.x, frame.max.y, Anchor::TopRight)
                .z_index(101);
        }
        Ok(())
    }

    /// Insert a named or anonymous interactive stop in the active segment.
    ///
    /// A stop at the segment's end keeps that completed segment active at the
    /// shared boundary until playback advances into the next segment. One
    /// authored while the segment's entry transition still runs waits for
    /// the transition to end, so a presentation never rests on a slide
    /// halfway in. After a recorded live take started, the stop becomes the
    /// pause recorded there.
    pub fn stop(&mut self, name: Option<String>) -> Result<(), SegmentError> {
        let name = match name {
            Some(name) => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return Err(SegmentError::EmptyStopName);
                }
                Some(name)
            }
            None => None,
        };
        let entering = {
            let state = self.state.lock().expect("canvas state poisoned");
            let segment = state.active();
            segment
                .transition
                .as_ref()
                .map_or(0.0, |transition| transition.duration() - segment.cursor)
        };
        if entering > 1e-9 {
            self.wait(entering);
        }
        if let Some(hold) = self.live_take_hold() {
            self.wait(hold);
            return Ok(());
        }
        let site = super::timeline_index::authoring_site();
        let mut state = self.state.lock().expect("canvas state poisoned");
        let segment = state.active_mut();
        let time = segment.cursor;
        if segment
            .stops
            .iter()
            .any(|stop| (stop.time - time).abs() < 1e-9)
        {
            return Err(SegmentError::DuplicateStopTime { time });
        }
        segment.stops.push(LocalSegmentStop {
            name,
            time,
            ambient: None,
        });
        segment.ops.push(Op::Stop);
        let key = (segment.id, segment.stops.len() - 1);
        if let Some(site) = site {
            state.authored_timeline.stops.insert(key, site);
        }
        Ok(())
    }

    /// Pause like [`Self::stop`], with an ambient loop: `composition` plays
    /// right after the stop and, while a presentation rests there, repeats
    /// until it advances, so a continuous motion keeps going while the
    /// speaker talks. An export plays it once. The loop should end where it
    /// starts for a seamless repeat.
    pub fn stop_with_loop(
        &mut self,
        name: Option<String>,
        composition: Composition,
    ) -> Result<(), StopLoopError> {
        let stops = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .stops
            .len();
        self.stop(name)?;
        let start = self.current_time();
        self.play_composition_configured(composition, None, None)?;
        let length = self.current_time() - start;
        if length <= 1.0e-9 {
            return Err(StopLoopError::Empty);
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        // A live narration take holds instead of stopping; it has no loop.
        if let Some(stop) = state.active_mut().stops.get_mut(stops) {
            stop.ambient = Some(length);
        }
        Ok(())
    }

    /// Name the current cursor on the global timeline (TM-05).
    ///
    /// Markers are pure metadata: they neither pause playback nor move the
    /// cursor. Names are unique, trimmed and must not parse as a number so
    /// `gaanim export --from/--to` can accept either seconds or a marker.
    pub fn marker(&mut self, name: impl Into<String>) -> Result<(), SegmentError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(SegmentError::EmptyMarkerName);
        }
        if name.parse::<f64>().is_ok() {
            return Err(SegmentError::NumericMarkerName { name });
        }
        if let Some(existing) = self
            .markers()
            .into_iter()
            .find(|marker| marker.name == name)
        {
            return Err(SegmentError::DuplicateMarker {
                name,
                time: existing.time,
            });
        }
        let site = super::timeline_index::authoring_site();
        let mut state = self.state.lock().expect("canvas state poisoned");
        let segment = state.active_mut();
        let time = segment.cursor;
        segment.markers.push((name.clone(), time));
        if let Some(site) = site {
            state.authored_timeline.markers.insert(name, site);
        }
        Ok(())
    }

    /// Markers authored so far, in timeline order, with absolute times.
    pub fn markers(&self) -> Vec<SceneMarker> {
        let state = self.state.lock().expect("canvas state poisoned");
        let mut start_time = 0.0;
        let mut markers = Vec::new();
        for segment in &state.segments {
            markers.extend(segment.markers.iter().map(|(name, time)| SceneMarker {
                name: name.clone(),
                time: start_time + time,
                segment: segment.name.clone(),
            }));
            start_time += segment.cursor;
        }
        markers.sort_by(|left, right| left.time.total_cmp(&right.time));
        markers
    }

    /// Absolute time of the marker named `name`, if it exists.
    pub fn marker_time(&self, name: &str) -> Option<f64> {
        self.markers()
            .into_iter()
            .find(|marker| marker.name == name.trim())
            .map(|marker| marker.time)
    }

    /// Return all segment metadata with local cursors converted to absolute time.
    pub fn segment_manifest(&self) -> SegmentManifest {
        let state = self.state.lock().expect("canvas state poisoned");
        let mut start_time = 0.0;
        let segments = state
            .segments
            .iter()
            .map(|segment| {
                let end_time = start_time + segment.cursor;
                let spec = SegmentSpec {
                    id: segment.id,
                    name: segment.name.clone(),
                    notes: self.slide_notes(&segment.name, segment.notes.as_ref()),
                    target: segment.target,
                    template: segment.template.clone(),
                    start_time,
                    end_time,
                    stops: segment
                        .stops
                        .iter()
                        .map(|stop| SegmentStop {
                            name: stop.name.clone(),
                            time: start_time + stop.time,
                            ambient: stop.ambient,
                        })
                        .collect(),
                };
                start_time = end_time;
                spec
            })
            .collect();
        SegmentManifest { segments }
    }

    /// Whether the canvas uses presentation-oriented segment features.
    pub fn has_presentation_features(&self) -> bool {
        self.branding.is_some()
            || self.segment_manifest().segments.iter().any(|segment| {
                segment.notes.is_some() || segment.template.is_some() || !segment.stops.is_empty()
            })
    }

    pub fn fade_out_all(&mut self, dur: f64) {
        let ids = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .mobject_ids
            .clone();
        let anims: Vec<AnimationBuilder> = ids
            .into_iter()
            .map(|id| AnimationBuilder {
                target: id,
                anim_type: AnimationType::FadeOut,
                duration: dur.max(0.0),
                rate_func: gaanim_math::RateFunc::Smooth,
                delay: 0.0,
            })
            .collect();
        if !anims.is_empty() {
            self.play_builders(anims);
        }
    }

    // -- Object controls --

    pub fn show(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Show(o.id));
    }

    pub fn hide(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Hide(o.id));
    }

    pub fn remove(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Remove(o.id));
    }

    /// Adopt an existing drawable into the active segment at the current cursor.
    pub fn reuse(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.reuse_many(std::slice::from_ref(o))
    }

    /// Adopt several existing drawables into the active segment.
    pub fn reuse_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Reuse)
    }

    /// Keep an existing drawable visible and animatable across future segments.
    pub fn persist(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.persist_many(std::slice::from_ref(o))
    }

    /// Name the scene's draw layers, from the back to the front. Every
    /// drawable of a layer draws above every drawable of the layers behind
    /// it, whatever their `z_index`, which only orders drawables within a
    /// layer. Drawables without a layer draw in `default`, the first layer
    /// when `None`. A scene names its layers once; naming the same layers
    /// again is allowed.
    pub fn z_layers(&mut self, names: &[&str], default: Option<&str>) -> Result<(), String> {
        if names.is_empty() {
            return Err("z_layers needs at least one layer name".to_string());
        }
        if let Some(name) = names.iter().find(|name| name.trim().is_empty()) {
            return Err(format!("layer names cannot be empty, got {name:?}"));
        }
        for (index, name) in names.iter().enumerate() {
            if names[..index].contains(name) {
                return Err(format!("layer `{name}` is named twice"));
            }
        }
        let default = match default {
            Some(default) => names
                .iter()
                .position(|name| *name == default)
                .ok_or_else(|| format!("default layer `{default}` is not one of the layers"))?,
            None => 0,
        };
        let layers = super::ops::ZLayers {
            names: names.iter().map(|name| (*name).to_string()).collect(),
            default,
        };
        let mut state = self.state.lock().expect("canvas state poisoned");
        match &state.z_layers {
            Some(declared) if *declared != layers => Err(format!(
                "the scene's layers are already {} (default {}); a scene names them once",
                declared.names.join(", "),
                declared.names[declared.default]
            )),
            _ => {
                state.z_layers = Some(layers);
                Ok(())
            }
        }
    }

    /// Create an empty 2.5D parallax layer at `depth` and return its root.
    ///
    /// Drawables added with [`Self::add_to_parallax_layer`] move by
    /// `1 / depth` of the orthographic camera's pan and zoom by
    /// `zoom^(1/depth)`: depth 1 is the scene plane, a larger depth is farther
    /// and moves less, a smaller one is nearer and moves more, and an infinite
    /// depth only follows the camera's roll. Farther layers draw behind nearer
    /// ones: the root's `z_index` defaults to `-100 ln(depth)`, within ±900,
    /// and its members add their own. A perspective camera leaves layers in
    /// place, since its own depth already gives parallax.
    pub fn parallax_layer(&mut self, depth: f64) -> Result<DrawableHandle, String> {
        if depth.is_nan() || depth <= 0.0 {
            return Err(format!(
                "layer depth must be positive (math.inf for a backdrop that never pans), got {depth}"
            ));
        }
        let layer = self.spawn(SpawnKind::GroupNoCenter(Vec::new()));
        {
            let mut spec = layer.spec.lock().expect("object spec poisoned");
            spec.parallax_depth = Some(depth);
            spec.z_index = (-100.0 * depth.ln()).round().clamp(-900.0, 900.0) as i32;
        }
        Ok(layer)
    }

    /// Move `member` into the parallax `layer`, keeping its current place.
    pub fn add_to_parallax_layer(
        &mut self,
        layer: &DrawableHandle,
        member: &DrawableHandle,
    ) -> Result<(), String> {
        if !Arc::ptr_eq(&layer.state, &self.state) || !layer.same_canvas(member) {
            return Err("layer members must belong to the layer's scene".to_string());
        }
        if layer
            .spec
            .lock()
            .expect("object spec poisoned")
            .parallax_depth
            .is_none()
        {
            return Err("only layers created with scene.layer() take members".to_string());
        }
        if member.id == layer.id {
            return Err("a layer cannot contain itself".to_string());
        }
        if member
            .spec
            .lock()
            .expect("object spec poisoned")
            .parallax_depth
            .is_some()
        {
            return Err("a layer cannot contain another layer".to_string());
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachToGroup {
                group: layer.id,
                child: member.id,
            });
        Ok(())
    }

    /// Keep several existing drawables visible and animatable across future segments.
    pub fn persist_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Persist)
    }

    /// Stop persisting a drawable and attach it to the active segment.
    pub fn release(&mut self, o: &DrawableHandle) -> Result<(), SceneObjectError> {
        self.release_many(std::slice::from_ref(o))
    }

    /// Stop persisting several drawables and attach them to the active segment.
    pub fn release_many(&mut self, objects: &[DrawableHandle]) -> Result<(), SceneObjectError> {
        self.queue_scene_objects(objects, SceneObjectAction::Release)
    }

    fn queue_scene_objects(
        &mut self,
        objects: &[DrawableHandle],
        action: SceneObjectAction,
    ) -> Result<(), SceneObjectError> {
        if objects.is_empty() {
            return Err(SceneObjectError::NoObjects);
        }
        if objects
            .iter()
            .any(|object| !Arc::ptr_eq(&self.state, &object.state))
        {
            return Err(SceneObjectError::ForeignScene);
        }

        let mut seen = HashSet::new();
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let segment = guard.active_mut();
        for object in objects {
            if !seen.insert(object.id) {
                continue;
            }
            if matches!(
                action,
                SceneObjectAction::Reuse | SceneObjectAction::Release
            ) && !segment.mobject_ids.contains(&object.id)
            {
                segment.mobject_ids.push(object.id);
            }
            segment.ops.push(match action {
                SceneObjectAction::Reuse => Op::Reuse(object.id),
                SceneObjectAction::Persist => Op::Persist(object.id),
                SceneObjectAction::Release => Op::Release(object.id),
            });
        }
        Ok(())
    }

    // -- Reactive objects --

    /// Spawn a value tracker — a reactive float signal that can be animated
    /// with `.animate().set(...)` and referenced by other reactive components.
    pub fn value_tracker(&mut self, initial: f64) -> DrawableHandle {
        self.spawn(SpawnKind::ValueTracker(initial))
    }

    /// Spawn a hidden dot that follows `curve` at the normalized value of
    /// `tracker`; reveal it with an entry animation in `SceneModel::play`.
    ///
    /// The tracker is clamped to `[0, 1]` and sampled by native arc length, so
    /// `0` is the first polyline point and `1` is the last one.
    pub fn point_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: impl Into<ScalarSource>,
    ) -> DrawableHandle {
        let handle = self.dot(0.08);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPointOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.into(),
            });
        handle
    }

    /// Spawn a hidden line centered and aligned with the tangent of `curve`.
    pub fn tangent_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: impl Into<ScalarSource>,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTangentOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.into(),
            });
        handle
    }

    /// Spawn a hidden line centered and perpendicular to the tangent of `curve`.
    pub fn normal_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: impl Into<ScalarSource>,
        length: f64,
    ) -> DrawableHandle {
        let half_length = length.max(0.0) / 2.0;
        let handle = self.line(-half_length, 0.0, half_length, 0.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachNormalOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.into(),
            });
        handle
    }

    /// Spawn a hidden unit circle scaled to the local osculating circle of `curve`.
    pub fn curvature_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: impl Into<ScalarSource>,
        window: f64,
    ) -> DrawableHandle {
        let handle = self.circle(1.0);
        handle.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCurvatureOnCurve {
                target: handle.id,
                curve: curve.id,
                tracker: tracker.into(),
                window,
            });
        handle
    }

    /// Creates a hidden curved arrow whose sweep is regenerated from `tracker`
    /// on every frame. The effective sweep is `value * sweep_scale + sweep_offset`.
    #[allow(clippy::too_many_arguments)]
    pub fn always_redraw_arc(
        &mut self,
        tracker: impl Into<ScalarSource>,
        cx: f64,
        cy: f64,
        radius: f64,
        start_angle: f64,
        initial_value: f64,
        sweep_scale: f64,
        sweep_offset: f64,
    ) -> DrawableHandle {
        let handle = self.curved_arrow_arc(
            cx,
            cy,
            radius,
            start_angle,
            initial_value * sweep_scale + sweep_offset,
        );
        handle.defer_visibility_until_play();
        let target = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackerArc {
                target,
                tracker: tracker.into(),
                center: (cx, cy),
                radius,
                start_angle,
                sweep_scale,
                sweep_offset,
            });
        handle
    }

    /// Spawn a hidden traced path that accumulates the trajectory of `source`
    /// as a continuous line. The returned drawable's Path2D is regenerated
    /// every frame and revealed by an entry animation in `SceneModel::play`.
    pub fn traced_path(&mut self, source: &DrawableHandle) -> DrawableHandle {
        self.traced_path_with_options(source, None, None, 1.0)
    }

    /// Spawn a traced path with optional temporal and sample-count retention limits.
    pub fn traced_path_with_options(
        &mut self,
        source: &DrawableHandle,
        dissipating_time: Option<f64>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPathLine);
        handle.defer_visibility_until_play();
        let source_id = source.id;
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTracedPath {
                target: id,
                source: source_id,
                min_distance,
                max_points,
                dissipating_time,
            });
        handle
    }

    /// Spawn a hidden 3D traced path that accumulates the 3D trajectory of
    /// `source` as a `LineList`.
    /// Supports any built-in or custom [`gaanim_core::ColorMap`] for time-based coloring.
    pub fn traced_path_3d(
        &mut self,
        source: &DrawableHandle,
        colormap: Option<gaanim_core::ColorMap>,
        max_points: Option<usize>,
        min_distance: f64,
    ) -> DrawableHandle {
        self.traced_path_3d_with_options(source, colormap, max_points, min_distance, None)
    }

    /// Spawn a 3D traced path with an optional temporal retention window.
    pub fn traced_path_3d_with_options(
        &mut self,
        source: &DrawableHandle,
        colormap: Option<gaanim_core::ColorMap>,
        max_points: Option<usize>,
        min_distance: f64,
        dissipating_time: Option<f64>,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPath3DLine);
        handle.defer_visibility_until_play();
        let source_id = source.id;
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTracedPath3D {
                target: id,
                source: source_id,
                min_distance: min_distance.max(0.0),
                max_points,
                colormap,
                dissipating_time,
            });
        handle
    }

    /// Attach a retained custom updater to `target`.
    #[doc(hidden)]
    pub fn attach_custom_updater(
        &mut self,
        target: &DrawableHandle,
        updater: gaanim_animation::Updater,
    ) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCustomUpdater {
                target: target.id,
                updater,
            });
    }

    /// Create a non-rendered point from two native scalar expressions.
    pub fn point_ref(&self, x: ScalarSource, y: ScalarSource) -> PointRef {
        PointRef(CanvasEndpoint::Expression {
            x,
            y,
            z: 0.0.into(),
        })
    }

    /// Create a non-rendered point displaced from an endpoint by reactive scene-space components.
    pub fn offset_point(
        &self,
        origin: CanvasEndpoint,
        dx: ScalarSource,
        dy: ScalarSource,
    ) -> PointRef {
        PointRef(CanvasEndpoint::Offset {
            origin: Box::new(origin),
            dx,
            dy,
        })
    }

    /// Create a non-rendered affine point between two reactive endpoints.
    pub fn point_between(
        &self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        alpha: impl Into<ScalarSource>,
        offset: DVec3,
    ) -> PointRef {
        PointRef(CanvasEndpoint::Between {
            from: Box::new(from),
            to: Box::new(to),
            alpha: alpha.into(),
            offset,
        })
    }

    /// Create a non-rendered polar point around another endpoint.
    pub fn polar_point(
        &self,
        origin: CanvasEndpoint,
        radius: ScalarSource,
        angle: ScalarSource,
    ) -> PointRef {
        PointRef(CanvasEndpoint::Polar {
            origin: Box::new(origin),
            radius,
            angle,
        })
    }

    fn mechanism_colors(&self, requested: Option<Color>) -> (Color, Color) {
        let background = self
            .background
            .or_else(|| self.theme_color("background").ok())
            .unwrap_or(Color::WHITE);
        let luminance = |color: Color| {
            let rgba = color.to_rgba8();
            0.2126 * f64::from(rgba.r) + 0.7152 * f64::from(rgba.g) + 0.0722 * f64::from(rgba.b)
        };
        let automatic = self.theme_color("foreground").unwrap_or(Color::BLACK);
        let foreground = requested.unwrap_or_else(|| {
            if (luminance(automatic) - luminance(background)).abs() >= 96.0 {
                automatic
            } else if luminance(background) < 128.0 {
                Color::WHITE
            } else {
                Color::BLACK
            }
        });
        (foreground, background)
    }

    fn annotation_text(
        &mut self,
        text: &str,
        font_size: Option<f64>,
        color: Option<Color>,
    ) -> Result<DrawableHandle, gaanim_text::prelude::TextSpecError> {
        let style = gaanim_text::prelude::TextStyle {
            size: Some(font_size.unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE)),
            color,
            ..gaanim_text::prelude::TextStyle::default()
        };
        gaanim_text::prelude::TextSpec::new(
            vec![text.into()],
            None,
            style,
            gaanim_text::prelude::TextFlow::default(),
        )
        .map(|spec| self.text_spec(spec))
    }

    /// Build a reactive angular technical dimension.
    pub fn angle_between_with_options(
        &mut self,
        vertex: CanvasEndpoint,
        from: CanvasRay,
        to: CanvasRay,
        radius: f64,
        options: AngleDimensionOptions,
    ) -> Result<AngleDimensionHandle, gaanim_text::prelude::TextSpecError> {
        let (color, _) = self.mechanism_colors(options.color);
        let font_size = Some(options.font_size.unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE));
        let arc = self
            .spawn(SpawnKind::TrackingLine)
            .no_fill()
            .stroke(color, 0.03);
        let arrows = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        let mut extensions = self
            .spawn(SpawnKind::TrackingLine)
            .no_fill()
            .stroke(color, 0.02);
        if !options.show_extensions {
            extensions = extensions.opacity(0.0);
        }
        for part in [&arc, &arrows, &extensions] {
            part.defer_visibility_until_play();
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingAngle {
                arc: arc.id,
                arrows: arrows.id,
                extensions: extensions.id,
                vertex: vertex.clone(),
                from: from.clone(),
                to: to.clone(),
                radius,
                sweep: options.sweep,
                arrowheads: options.arrowheads,
            });

        let label = options
            .label
            .as_deref()
            .map(|text| self.annotation_text(text, font_size, Some(color)))
            .transpose()?;
        let mut number = None;
        let mut unit = None;
        let annotation = if options.show_value {
            let tracker = self.value_tracker(0.0);
            let scale = if options.unit == "deg" {
                180.0 / std::f64::consts::PI
            } else {
                1.0
            };
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::AttachEndpointAngle {
                    target: tracker.id,
                    vertex: vertex.clone(),
                    from: from.clone(),
                    to: to.clone(),
                    sweep: options.sweep,
                    scale,
                });
            let number_handle = self
                .reactive_readout(
                    ScalarSource::signal(tracker.id),
                    options.format.clone(),
                    "",
                    "",
                    "—",
                    font_size,
                )
                .fill(color);
            let equals = label
                .as_ref()
                .map(|_| self.annotation_text("=", font_size, Some(color)))
                .transpose()?;
            let unit_text = if options.unit == "deg" { "°" } else { "rad" };
            let unit_handle = self.annotation_text(unit_text, font_size, Some(color))?;
            let group = self.reactive_readout_group(
                label.as_ref(),
                equals.as_ref(),
                &number_handle,
                Some(&unit_handle),
                0.08,
            );
            number = Some(number_handle);
            unit = Some(unit_handle);
            Some(group)
        } else {
            label.clone()
        };

        if let Some(annotation) = &annotation {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::AttachAngleLabelPlacement {
                    target: arc.id,
                    label: annotation.id,
                    vertex,
                    from,
                    to,
                    radius,
                    gap: options.label_gap,
                    sweep: options.sweep,
                    orientation: options.label_orientation,
                });
        }
        let mut members = vec![&extensions, &arc, &arrows];
        if let Some(annotation) = &annotation {
            members.push(annotation);
        }
        let drawable = self.group_no_center(&members);
        Ok(AngleDimensionHandle {
            drawable,
            arc,
            arrows,
            extensions,
            label,
            number,
            unit,
        })
    }

    /// Create a reactive vector with a solid head and optional magnitude readout.
    #[allow(clippy::too_many_arguments)]
    pub fn vector_between_with_parts(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        label_text: Option<String>,
        show_value: bool,
        format: String,
        unit_text: Option<String>,
        scale: f64,
        label_gap: f64,
        font_size: Option<f64>,
        requested_color: Option<Color>,
    ) -> Result<ForceVectorHandle, gaanim_text::prelude::TextSpecError> {
        let (color, _) = self.mechanism_colors(requested_color);
        let font_size = Some(font_size.unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE));
        let shaft = self
            .tracking_line(from.clone(), to.clone())
            .no_fill()
            .stroke(color, 0.04);
        let head = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        head.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingVectorHead {
                target: head.id,
                from: from.clone(),
                to: to.clone(),
                length: 0.16,
                width: 0.12,
            });
        let label = label_text
            .as_deref()
            .map(|text| self.annotation_text(text, font_size, Some(color)))
            .transpose()?;
        let mut number_part = None;
        let mut unit_part = None;
        let annotation = if show_value {
            let tracker = self.value_tracker(0.0);
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::AttachEndpointDistance {
                    target: tracker.id,
                    from: from.clone(),
                    to: to.clone(),
                    scale,
                });
            let number = self
                .reactive_readout(
                    ScalarSource::signal(tracker.id),
                    format,
                    "",
                    "",
                    "—",
                    font_size,
                )
                .fill(color);
            let equals = label
                .as_ref()
                .map(|_| self.annotation_text("=", font_size, Some(color)))
                .transpose()?;
            let unit = unit_text
                .as_deref()
                .map(|text| self.annotation_text(text, font_size, Some(color)))
                .transpose()?;
            let group = self.reactive_readout_group(
                label.as_ref(),
                equals.as_ref(),
                &number,
                unit.as_ref(),
                0.08,
            );
            number_part = Some(number);
            unit_part = unit;
            Some(group)
        } else {
            label.clone()
        };
        if let Some(annotation) = &annotation {
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::AttachDimensionLabelPlacement {
                    target: shaft.id,
                    label: annotation.id,
                    from,
                    to,
                    offset: 0.0,
                    side: None,
                    gap: label_gap,
                    orientation: gaanim_animation::DimensionLabelOrientation::Upright,
                    clear_label_width: false,
                });
        }
        let mut members = vec![&shaft, &head];
        if let Some(annotation) = &annotation {
            members.push(annotation);
        }
        let drawable = self.group_no_center(&members);
        Ok(ForceVectorHandle {
            drawable,
            shaft,
            head,
            label,
            number: number_part,
            unit: unit_part,
        })
    }

    /// Create a reactive vector with a solid head and optional magnitude readout.
    #[allow(clippy::too_many_arguments)]
    pub fn vector_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        label_text: Option<String>,
        show_value: bool,
        format: String,
        unit_text: Option<String>,
        scale: f64,
        label_gap: f64,
        font_size: Option<f64>,
        requested_color: Option<Color>,
    ) -> Result<DrawableHandle, gaanim_text::prelude::TextSpecError> {
        self.vector_between_with_parts(
            from,
            to,
            label_text,
            show_value,
            format,
            unit_text,
            scale,
            label_gap,
            font_size,
            requested_color,
        )
        .map(|force| force.drawable)
    }

    /// Create a force from a reactive physical magnitude and direction in radians.
    #[allow(clippy::too_many_arguments)]
    pub fn force_at(
        &mut self,
        origin: CanvasEndpoint,
        magnitude: ScalarSource,
        direction: ScalarSource,
        visual_scale: f64,
        label_text: Option<String>,
        show_value: bool,
        format: String,
        unit_text: Option<String>,
        label_gap: f64,
        font_size: Option<f64>,
        requested_color: Option<Color>,
    ) -> Result<ForceVectorHandle, gaanim_text::prelude::TextSpecError> {
        let tip = CanvasEndpoint::Polar {
            origin: Box::new(origin.clone()),
            radius: magnitude.scaled(visual_scale),
            angle: direction,
        };
        self.vector_between_with_parts(
            origin,
            tip,
            label_text,
            show_value,
            format,
            unit_text,
            1.0 / visual_scale,
            label_gap,
            font_size,
            requested_color,
        )
    }

    /// Create a force from reactive physical X/Y components.
    #[allow(clippy::too_many_arguments)]
    pub fn force_from_components(
        &mut self,
        origin: CanvasEndpoint,
        fx: ScalarSource,
        fy: ScalarSource,
        visual_scale: f64,
        label_text: Option<String>,
        show_value: bool,
        format: String,
        unit_text: Option<String>,
        label_gap: f64,
        font_size: Option<f64>,
        requested_color: Option<Color>,
    ) -> Result<ForceVectorHandle, gaanim_text::prelude::TextSpecError> {
        let tip = CanvasEndpoint::Offset {
            origin: Box::new(origin.clone()),
            dx: fx.scaled(visual_scale),
            dy: fy.scaled(visual_scale),
        };
        self.vector_between_with_parts(
            origin,
            tip,
            label_text,
            show_value,
            format,
            unit_text,
            1.0 / visual_scale,
            label_gap,
            font_size,
            requested_color,
        )
    }

    fn transform_symbol_point(point: DVec2, endpoint: DVec2, direction: DVec2) -> DVec2 {
        let direction = direction.normalize_or_zero();
        let tangent = DVec2::new(-direction.y, direction.x);
        endpoint + tangent * point.x + direction * point.y
    }

    /// Create a polished, vector-native structural support that follows an endpoint.
    pub fn support_at(
        &mut self,
        point: CanvasEndpoint,
        kind: &str,
        direction: DVec3,
        size: f64,
        ground_length: f64,
        requested_color: Option<Color>,
    ) -> SupportHandle {
        let (foreground, background) = self.mechanism_colors(requested_color);
        let s = size;
        let ground_y = -s * 0.82;
        let transform =
            |xy: DVec2| Self::transform_symbol_point(xy, DVec2::ZERO, direction.truncate());
        let line_from_points =
            |canvas: &mut SceneModel, points: &[DVec2], width: f64, color: Color| {
                let points = points
                    .iter()
                    .map(|point| {
                        let p = transform(*point);
                        (p.x, p.y)
                    })
                    .collect::<Vec<_>>();
                canvas.polyline(&points).no_fill().stroke(color, width)
            };
        let polygon_from_points = |canvas: &mut SceneModel, points: &[DVec2]| {
            let points = points
                .iter()
                .map(|point| {
                    let p = transform(*point);
                    (p.x, p.y)
                })
                .collect::<Vec<_>>();
            canvas
                .polygon(points)
                .fill(background)
                .stroke(foreground, s * 0.075)
        };

        // A fixed support connects directly to its plate; drawing a pin at the
        // attachment would communicate the wrong joint.
        let joint = if kind == "fixed" {
            self.group_no_center(&[])
        } else {
            self.dot(s * 0.13)
                .fill(background)
                .stroke(foreground, s * 0.075)
        };
        let mut body_parts = Vec::new();
        let mut roller_parts = Vec::new();
        let mut guide_parts = Vec::new();
        let mut hatch_parts = Vec::new();
        let mut ground_parts = Vec::new();

        match kind {
            "fixed" => {
                // The fixed boundary itself is the connection point. Connected
                // members therefore terminate directly on the plate instead of
                // on a short stem that can be mistaken for another member.
                let plate_y = 0.0;
                let plate = line_from_points(
                    self,
                    &[
                        DVec2::new(-ground_length * 0.5, plate_y),
                        DVec2::new(ground_length * 0.5, plate_y),
                    ],
                    s * 0.10,
                    foreground,
                );
                ground_parts.push(plate);
                for x in (-3..=3).map(|index| index as f64 * ground_length / 7.0) {
                    hatch_parts.push(line_from_points(
                        self,
                        &[
                            DVec2::new(x - s * 0.16, plate_y - s * 0.22),
                            DVec2::new(x + s * 0.10, plate_y - s * 0.02),
                        ],
                        s * 0.045,
                        foreground,
                    ));
                }
            }
            "pin" | "roller" => {
                body_parts.push(polygon_from_points(
                    self,
                    &[
                        DVec2::ZERO,
                        DVec2::new(-s * 0.42, ground_y + s * 0.12),
                        DVec2::new(s * 0.42, ground_y + s * 0.12),
                    ],
                ));
                let roller_shift = if kind == "roller" { s * 0.19 } else { 0.0 };
                if kind == "roller" {
                    for x in [-s * 0.24, s * 0.24] {
                        let center = transform(DVec2::new(x, ground_y));
                        roller_parts.push(
                            self.dot(s * 0.11)
                                .fill(background)
                                .stroke(foreground, s * 0.055)
                                .move_to(center.x, center.y),
                        );
                    }
                }
                let base_y = ground_y - roller_shift;
                ground_parts.push(line_from_points(
                    self,
                    &[
                        DVec2::new(-ground_length * 0.5, base_y),
                        DVec2::new(ground_length * 0.5, base_y),
                    ],
                    s * 0.07,
                    foreground,
                ));
                for x in (-3..=3).map(|index| index as f64 * ground_length / 7.0) {
                    hatch_parts.push(line_from_points(
                        self,
                        &[
                            DVec2::new(x - s * 0.14, base_y - s * 0.20),
                            DVec2::new(x + s * 0.10, base_y - s * 0.02),
                        ],
                        s * 0.04,
                        foreground,
                    ));
                }
            }
            "simple" => {
                roller_parts.push(
                    self.dot(s * 0.13)
                        .fill(background)
                        .stroke(foreground, s * 0.06),
                );
                ground_parts.push(line_from_points(
                    self,
                    &[
                        DVec2::new(-ground_length * 0.5, -s * 0.28),
                        DVec2::new(ground_length * 0.5, -s * 0.28),
                    ],
                    s * 0.07,
                    foreground,
                ));
            }
            "guided" | "prismatic" => {
                let center = transform(DVec2::new(0.0, -s * 0.34));
                body_parts.push(
                    self.rounded_rect(s * 0.72, s * 0.42, s * 0.08)
                        .fill(background)
                        .stroke(foreground, s * 0.065)
                        .move_to(center.x, center.y)
                        .rotate_to(direction.y.atan2(direction.x) - std::f64::consts::FRAC_PI_2),
                );
                if kind == "guided" {
                    for x in [-s * 0.48, s * 0.48] {
                        guide_parts.push(line_from_points(
                            self,
                            &[DVec2::new(x, -s * 0.72), DVec2::new(x, s * 0.04)],
                            s * 0.055,
                            foreground,
                        ));
                    }
                } else {
                    guide_parts.push(line_from_points(
                        self,
                        &[
                            DVec2::new(-s * 0.82, -s * 0.34),
                            DVec2::new(s * 0.82, -s * 0.34),
                        ],
                        s * 0.055,
                        foreground,
                    ));
                }
            }
            "cable" => {
                body_parts.push(line_from_points(
                    self,
                    &[DVec2::ZERO, DVec2::new(0.0, -s * 0.88)],
                    s * 0.075,
                    foreground,
                ));
                let lower = transform(DVec2::new(0.0, -s * 0.88));
                roller_parts.push(
                    self.dot(s * 0.10)
                        .fill(background)
                        .stroke(foreground, s * 0.055)
                        .move_to(lower.x, lower.y),
                );
            }
            "spring" => {
                let mut points = vec![DVec2::ZERO, DVec2::new(0.0, -s * 0.12)];
                for index in 0..=10 {
                    let y = -s * 0.12 + (ground_y + s * 0.24) * index as f64 / 10.0;
                    let x = if index == 0 || index == 10 {
                        0.0
                    } else if index % 2 == 0 {
                        s * 0.19
                    } else {
                        -s * 0.19
                    };
                    points.push(DVec2::new(x, y));
                }
                points.push(DVec2::new(0.0, ground_y));
                body_parts.push(line_from_points(self, &points, s * 0.055, foreground));
                ground_parts.push(line_from_points(
                    self,
                    &[
                        DVec2::new(-ground_length * 0.5, ground_y),
                        DVec2::new(ground_length * 0.5, ground_y),
                    ],
                    s * 0.07,
                    foreground,
                ));
                for x in (-3..=3).map(|index| index as f64 * ground_length / 7.0) {
                    hatch_parts.push(line_from_points(
                        self,
                        &[
                            DVec2::new(x - s * 0.14, ground_y - s * 0.20),
                            DVec2::new(x + s * 0.10, ground_y - s * 0.02),
                        ],
                        s * 0.04,
                        foreground,
                    ));
                }
            }
            _ => {}
        }

        let empty_group = |canvas: &mut SceneModel| canvas.group_no_center(&[]);
        let body_refs = body_parts.iter().collect::<Vec<_>>();
        let ground_refs = ground_parts.iter().collect::<Vec<_>>();
        let roller_refs = roller_parts.iter().collect::<Vec<_>>();
        let guide_refs = guide_parts.iter().collect::<Vec<_>>();
        let hatch_refs = hatch_parts.iter().collect::<Vec<_>>();
        let body = if body_refs.is_empty() {
            empty_group(self)
        } else {
            self.group_no_center(&body_refs)
        };
        let ground = if ground_refs.is_empty() {
            empty_group(self)
        } else {
            self.group_no_center(&ground_refs)
        };
        let rollers = if roller_refs.is_empty() {
            empty_group(self)
        } else {
            self.group_no_center(&roller_refs)
        };
        let guides = if guide_refs.is_empty() {
            empty_group(self)
        } else {
            self.group_no_center(&guide_refs)
        };
        let hatching = if hatch_refs.is_empty() {
            empty_group(self)
        } else {
            self.group_no_center(&hatch_refs)
        };
        let drawable =
            self.group_no_center(&[&ground, &hatching, &guides, &body, &rollers, &joint]);
        match point {
            CanvasEndpoint::Static(position) => {
                drawable
                    .clone()
                    .move_to_3d(position.x, position.y, position.z);
            }
            point => {
                drawable.follow_endpoint(
                    point,
                    DVec3::ZERO,
                    gaanim_animation::FollowOffsetSpace::World,
                );
            }
        }
        SupportHandle {
            drawable,
            joint,
            body,
            ground,
            rollers,
            guides,
            hatching,
        }
    }

    /// Create a standalone revolute or prismatic joint symbol.
    pub fn joint_at(
        &mut self,
        point: CanvasEndpoint,
        kind: &str,
        axis: DVec3,
        size: f64,
        requested_color: Option<Color>,
    ) -> DrawableHandle {
        let (foreground, background) = self.mechanism_colors(requested_color);
        let joint = match kind {
            "prismatic" => self
                .rounded_rect(size, size * 0.56, size * 0.1)
                .fill(background)
                .stroke(foreground, size * 0.08)
                .rotate_to(axis.y.atan2(axis.x)),
            _ => self
                .dot(size * 0.28)
                .fill(background)
                .stroke(foreground, size * 0.10),
        };
        joint.follow_endpoint(
            point,
            DVec3::ZERO,
            gaanim_animation::FollowOffsetSpace::World,
        )
    }

    /// Create an editorial gear silhouette with equally spaced teeth.
    pub fn gear(
        &mut self,
        radius: f64,
        teeth: usize,
        bore_radius: f64,
        requested_color: Option<Color>,
    ) -> DrawableHandle {
        let (foreground, background) = self.mechanism_colors(requested_color);
        let mut points = Vec::with_capacity(teeth * 4);
        for index in 0..teeth * 4 {
            let angle = std::f64::consts::TAU * index as f64 / (teeth * 4) as f64;
            let r = if index % 4 == 1 || index % 4 == 2 {
                radius * 1.13
            } else {
                radius
            };
            points.push((r * angle.cos(), r * angle.sin()));
        }
        let rim = self
            .polygon(points)
            .fill(background)
            .stroke(foreground, (radius * 0.06).clamp(0.02, 0.06));
        let bore = self
            .dot(bore_radius)
            .fill(background)
            .stroke(foreground, (radius * 0.05).clamp(0.02, 0.05));
        self.group_no_center(&[&rim, &bore])
    }

    /// Create an editorial straight rack with trapezoidal teeth.
    pub fn rack(
        &mut self,
        length: f64,
        teeth: usize,
        requested_color: Option<Color>,
    ) -> DrawableHandle {
        let (foreground, background) = self.mechanism_colors(requested_color);
        let pitch = length / teeth as f64;
        let height = pitch * 1.25;
        let mut points = vec![(-length * 0.5, -height)];
        for tooth in 0..teeth {
            let x = -length * 0.5 + tooth as f64 * pitch;
            points.extend([
                (x, 0.0),
                (x + pitch * 0.25, height * 0.45),
                (x + pitch * 0.75, height * 0.45),
                (x + pitch, 0.0),
            ]);
        }
        points.push((length * 0.5, -height));
        self.polygon(points)
            .fill(background)
            .stroke(foreground, (pitch * 0.18).clamp(0.02, 0.05))
    }

    /// Create a closed radial cam profile from `(angle, radius)` samples.
    pub fn cam_profile(
        &mut self,
        samples: &[(f64, f64)],
        bore_radius: f64,
        requested_color: Option<Color>,
    ) -> DrawableHandle {
        let (foreground, background) = self.mechanism_colors(requested_color);
        let points = samples
            .iter()
            .map(|(angle, radius)| (radius * angle.cos(), radius * angle.sin()))
            .collect();
        let profile = self
            .polygon(points)
            .fill(background)
            .stroke(foreground, 0.04);
        let bore = self
            .dot(bore_radius)
            .fill(background)
            .stroke(foreground, 0.03);
        self.group_no_center(&[&profile, &bore])
    }

    /// Group point, tangent, and normal helpers for a curve contact visualization.
    pub fn contact_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: impl Into<ScalarSource>,
        tangent_length: f64,
        normal_length: f64,
    ) -> DrawableHandle {
        let tracker = tracker.into();
        let point = self.point_on_curve(curve, tracker.clone());
        let tangent = self.tangent_on_curve(curve, tracker.clone(), tangent_length);
        let normal = self.normal_on_curve(curve, tracker, normal_length);
        self.group_no_center(&[&tangent, &normal, &point])
    }

    /// Create a curved moment arrow around a reactive center.
    pub fn moment_about(
        &mut self,
        center: CanvasEndpoint,
        radius: f64,
        counter_clockwise: bool,
        label: Option<String>,
        requested_color: Option<Color>,
    ) -> Result<DrawableHandle, gaanim_text::prelude::TextSpecError> {
        let (color, _) = self.mechanism_colors(requested_color);
        let sweep = if counter_clockwise {
            std::f64::consts::PI * 1.55
        } else {
            -std::f64::consts::PI * 1.55
        };
        let arrow = self
            .curved_arrow_arc(0.0, 0.0, radius, -std::f64::consts::FRAC_PI_2, sweep)
            .fill(color)
            .no_stroke();
        arrow.follow_endpoint(
            center.clone(),
            DVec3::ZERO,
            gaanim_animation::FollowOffsetSpace::World,
        );
        let Some(text) = label else {
            return Ok(arrow);
        };
        let annotation = self.annotation_text(&text, None, Some(color))?;
        annotation.follow_endpoint(
            center,
            DVec3::new(0.0, radius + 0.18, 0.0),
            gaanim_animation::FollowOffsetSpace::World,
        );
        Ok(self.group_no_center(&[&arrow, &annotation]))
    }

    /// Create two reactive orthogonal coordinate-frame arrows from an origin.
    pub fn coordinate_frame_at(
        &mut self,
        origin: CanvasEndpoint,
        x_direction: DVec3,
        length: f64,
        labels: Option<(String, String)>,
        requested_color: Option<Color>,
    ) -> Result<DrawableHandle, gaanim_text::prelude::TextSpecError> {
        let x = x_direction.truncate().normalize_or_zero();
        let y = DVec2::new(-x.y, x.x);
        let x_tip = PointRef(CanvasEndpoint::Between {
            from: Box::new(origin.clone()),
            to: Box::new(origin.clone()),
            alpha: 0.0.into(),
            offset: DVec3::new(x.x * length, x.y * length, 0.0),
        });
        let y_tip = PointRef(CanvasEndpoint::Between {
            from: Box::new(origin.clone()),
            to: Box::new(origin.clone()),
            alpha: 0.0.into(),
            offset: DVec3::new(y.x * length, y.y * length, 0.0),
        });
        let x_arrow = self.vector_between(
            origin.clone(),
            x_tip.0.clone(),
            None,
            false,
            ".1f".into(),
            None,
            1.0,
            8.0,
            None,
            requested_color,
        )?;
        let y_arrow = self.vector_between(
            origin.clone(),
            y_tip.0.clone(),
            None,
            false,
            ".1f".into(),
            None,
            1.0,
            8.0,
            None,
            requested_color,
        )?;
        let mut members = vec![&x_arrow, &y_arrow];
        let mut label_handles = Vec::new();
        if let Some((x_label, y_label)) = labels {
            let (color, _) = self.mechanism_colors(requested_color);
            let lx = self.annotation_text(&x_label, None, Some(color))?;
            lx.follow_endpoint(
                x_tip.0,
                DVec3::new(x.x * 14.0, x.y * 14.0, 0.0),
                gaanim_animation::FollowOffsetSpace::World,
            );
            let ly = self.annotation_text(&y_label, None, Some(color))?;
            ly.follow_endpoint(
                y_tip.0,
                DVec3::new(y.x * 14.0, y.y * 14.0, 0.0),
                gaanim_animation::FollowOffsetSpace::World,
            );
            label_handles.extend([lx, ly]);
        }
        for label in &label_handles {
            members.push(label);
        }
        Ok(self.group_no_center(&members))
    }

    /// Spawn a hidden tracking line — a reactive line whose endpoints follow
    /// entities or remain at fixed positions. Updated every frame and revealed
    /// by an entry animation in `SceneModel::play`.
    ///
    /// Endpoints can be `DrawableHandle` references (their `.id` is used) or
    /// static `(f64, f64)` positions passed as tuples.
    pub fn tracking_line(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        self.endpoint_line(from, to, true)
    }

    /// A filled polyline arrow whose endpoints and waypoints follow their references.
    /// Dimensions are world units; the head is capped to the last nonzero segment.
    #[allow(clippy::too_many_arguments)]
    pub fn connector(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        via: Vec<CanvasEndpoint>,
        head_length: f64,
        head_width: f64,
        body_width: f64,
        max_head_ratio: Option<f64>,
    ) -> Result<DrawableHandle, &'static str> {
        if [head_length, head_width, body_width]
            .iter()
            .any(|x| !x.is_finite() || *x <= 0.0)
            || max_head_ratio.is_some_and(|x| !x.is_finite() || x <= 0.0 || x > 1.0)
        {
            return Err(
                "connector dimensions must be finite and positive; max_head_ratio must be in (0, 1]",
            );
        }
        let mut points = vec![from];
        points.extend(via);
        points.push(to);
        {
            let state = self.state.lock().expect("canvas state poisoned");
            if !points.iter().all(|p| endpoint_is_valid(p, &state)) {
                return Err("connector endpoints must be finite and belong to this Scene");
            }
        }
        let color = self.theme_color("foreground").unwrap_or(Color::BLACK);
        let handle = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingConnector {
                target: handle.id,
                points,
                head_length,
                head_width,
                body_width,
                max_head_ratio,
            });
        Ok(handle)
    }

    pub(crate) fn endpoint_line(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        defer_visibility: bool,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        if defer_visibility {
            handle.defer_visibility_until_play();
        }
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingLine {
                target: id,
                from,
                to,
                dashes: None,
            });
        handle
    }

    /// Spawn a thick, round-capped reactive bar between two endpoints.
    pub fn bar_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        width: f64,
    ) -> DrawableHandle {
        let mut style = Stroke::new(width);
        style.start_cap = Cap::Round;
        style.end_cap = Cap::Round;
        self.tracking_line(from, to)
            .stroke_with_style(Brush::Solid(Color::BLACK), style)
    }

    /// Spawn a hidden reactive helical spring between two endpoints.
    ///
    /// Each endpoint can be static or follow a drawable. The path is rebuilt
    /// natively after updaters and position bindings have run, changing the
    /// coil pitch as the endpoint distance changes.
    pub fn spring_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
    ) -> DrawableHandle {
        self.spring_between_with_crossing(from, to, coils, amplitude, 0.0)
    }

    /// Spawn a reactive helical spring with optional e-like turn crossings.
    ///
    /// `crossing` is clamped to `[0, 1]`: zero produces ordinary sinusoidal
    /// coils, while one folds each turn back along the spring axis briefly.
    pub fn spring_between_with_crossing(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
        crossing: f64,
    ) -> DrawableHandle {
        self.spring_between_with_options(
            from,
            to,
            coils,
            amplitude,
            crossing,
            DEFAULT_SPRING_STRAIGHT,
            DEFAULT_SPRING_STRAIGHT,
        )
    }

    /// Spawn a reactive helical spring with configurable straight end segments.
    ///
    /// The straight lengths are measured from `from` and `to` toward the coil.
    /// They are proportionally shortened when the endpoints are too close to
    /// accommodate both segments and a coil.
    #[allow(clippy::too_many_arguments)]
    pub fn spring_between_with_options(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
        crossing: f64,
        start_straight: f64,
        end_straight: f64,
    ) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
        handle.defer_visibility_until_play();
        let id = handle.id;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingSpring {
                target: id,
                from,
                to,
                coils,
                amplitude,
                crossing,
                start_straight,
                end_straight,
            });
        handle
    }

    /// Spawn a hidden reactive dimension line between two endpoints.
    pub fn dimension_between(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
    ) -> DrawableHandle {
        let (line, extensions, drawable) =
            self.dimension_between_parts(from, to, offset, None, 0.03, None, Color::WHITE);
        let _ = (line, extensions);
        drawable
    }

    #[allow(clippy::too_many_arguments)]
    fn dimension_between_parts(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
        side: Option<gaanim_animation::DimensionSide>,
        line_width: f64,
        extension_dash: Option<(f64, f64)>,
        color: Color,
    ) -> (DrawableHandle, DrawableHandle, DrawableHandle) {
        // Like any annotation, a dimension is visible from its declaration
        // unless an entry animation reveals it.
        let line = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        let extensions = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachTrackingDimension {
                line: line.id,
                extensions: extensions.id,
                from,
                to,
                offset,
                side,
                line_width,
                extension_dash,
            });
        let drawable = self.group_no_center(&[&extensions, &line]);
        (line, extensions, drawable)
    }

    /// Build a reactive dimension with an optional symbolic and numeric annotation.
    pub fn dimension_between_with_options(
        &mut self,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
        options: DimensionOptions,
    ) -> Result<DimensionHandle, gaanim_text::prelude::TextSpecError> {
        let color = options.color.unwrap_or(Color::WHITE);
        let extension_dash = (options.extension_style == DimensionExtensionStyle::Dashed)
            .then_some((options.dash_length, options.gap_length));
        let (measure, extensions, visual) = self.dimension_between_parts(
            from.clone(),
            to.clone(),
            offset,
            options.side,
            options.line_width,
            extension_dash,
            color,
        );
        let explicit_value = options.value.clone();
        if options.label.is_none() && !options.show_value && explicit_value.is_none() {
            return Ok(DimensionHandle {
                drawable: visual.clone(),
                line: visual,
                extensions,
                label: None,
                number: None,
                unit: None,
            });
        }

        let text_size = options
            .font_size
            .or(options.label_style.size)
            .unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE);
        let text_color = options.label_style.color.or(options.color);
        let text_part = |canvas: &mut SceneModel, text: &str| {
            let mut style = options.label_style.clone();
            style.size = Some(text_size);
            style.color = text_color;
            gaanim_text::prelude::TextSpec::new(
                vec![text.into()],
                None,
                style,
                gaanim_text::prelude::TextFlow::default(),
            )
            .map(|spec| canvas.text_spec(spec))
        };

        let label = options
            .label
            .as_deref()
            .map(|text| text_part(self, text))
            .transpose()?;
        let mut number = None;
        let mut unit = None;
        let annotation = if options.show_value || explicit_value.is_some() {
            let value_expr = if let Some(value) = explicit_value {
                value
            } else {
                let tracker = self.value_tracker(0.0);
                self.state
                    .lock()
                    .expect("canvas state poisoned")
                    .active_mut()
                    .ops
                    .push(Op::AttachEndpointDistance {
                        target: tracker.id,
                        from: from.clone(),
                        to: to.clone(),
                        scale: options.scale,
                    });
                ScalarSource::signal(tracker.id)
            };
            let mut number_handle = self.reactive_readout_with_font(
                value_expr,
                options.format.clone(),
                "",
                "",
                "—",
                Some(text_size),
                options.label_style.font.clone(),
                options.label_style.weight,
            );
            if let Some(color) = text_color {
                number_handle = number_handle.fill(color);
            }
            let equals = label.as_ref().map(|_| text_part(self, "=")).transpose()?;
            let unit_handle = options
                .unit
                .as_deref()
                .map(|text| text_part(self, text))
                .transpose()?;
            let group = self.reactive_readout_group(
                label.as_ref(),
                equals.as_ref(),
                &number_handle,
                unit_handle.as_ref(),
                0.1,
            );
            number = Some(number_handle);
            unit = unit_handle;
            group
        } else {
            label
                .as_ref()
                .expect("annotation exists when label is present")
                .clone()
        };

        let drawable = self.group_no_center(&[&visual, &annotation]);
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachDimensionLabelPlacement {
                target: measure.id,
                label: annotation.id,
                from,
                to,
                offset,
                side: options.side,
                gap: options.label_gap,
                orientation: options.label_orientation,
                clear_label_width: true,
            });

        Ok(DimensionHandle {
            drawable,
            line: visual,
            extensions,
            label,
            number,
            unit,
        })
    }

    // -- Render / export --

    /// Submit the scene to the host. Narration takes still playing at the
    /// cursor extend the timeline until they end.
    pub fn render(&self) -> bool {
        // Authoring is over; measurements no longer need their compilation.
        self.profile.rendered();
        self.measured.clear();
        let mut scene = self.clone();
        scene.measured = Default::default();
        scene.complete_narration();
        crate::host::send_to_host(scene)
    }

    pub fn export(
        &self,
        path: &str,
        fps: Option<u32>,
        _encoder: Option<&str>,
        transparent: Option<bool>,
    ) -> Result<(), gaanim_export::encoder::ExportError> {
        let mut scene = self.clone();
        scene.complete_narration();
        crate::export::export_canvas_to_path(scene, path, fps, transparent)
    }
}

/// Whether `point` is finite and refers only to drawables and sources of the
/// scene `state` describes.
fn endpoint_is_valid(point: &CanvasEndpoint, state: &CanvasState) -> bool {
    let scalar = |s: &ScalarSource| {
        s.scene_owners().iter().all(|id| *id == state.scene_id)
            && s.constant_value().is_none_or(f64::is_finite)
    };
    match point {
        CanvasEndpoint::Static(p) => p.is_finite(),
        CanvasEndpoint::Entity(id) => state.object_specs.contains_key(id),
        CanvasEndpoint::Anchor(p) => {
            p.scene_id == state.scene_id
                && state.object_specs.contains_key(&p.object)
                && p.normalized.is_finite()
                && p.offset.is_finite()
        }
        CanvasEndpoint::Expression { x, y, z } => scalar(x) && scalar(y) && scalar(z),
        CanvasEndpoint::MediaPixel { object, x, y } => {
            state.object_specs.contains_key(object) && x.is_finite() && y.is_finite()
        }
        CanvasEndpoint::LocalExpression { space, x, y, z } => {
            state.object_specs.contains_key(space) && scalar(x) && scalar(y) && scalar(z)
        }
        CanvasEndpoint::LocalNumberLine {
            space,
            value,
            normal_offset,
            ..
        } => state.object_specs.contains_key(space) && scalar(value) && scalar(normal_offset),
        CanvasEndpoint::Offset { origin, dx, dy } => {
            endpoint_is_valid(origin, state) && scalar(dx) && scalar(dy)
        }
        CanvasEndpoint::Between {
            from,
            to,
            alpha,
            offset,
        } => {
            endpoint_is_valid(from, state)
                && endpoint_is_valid(to, state)
                && scalar(alpha)
                && offset.is_finite()
        }
        CanvasEndpoint::Polar {
            origin,
            radius,
            angle,
        } => endpoint_is_valid(origin, state) && scalar(radius) && scalar(angle),
        CanvasEndpoint::Rim {
            object,
            toward,
            radius,
        } => {
            state.object_specs.contains_key(object)
                && endpoint_is_valid(toward, state)
                && radius.is_finite()
        }
        CanvasEndpoint::Screen(point) => endpoint_is_valid(point, state),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod scene_unit_default_tests;

#[cfg(test)]
mod grow_arrow_tests;
