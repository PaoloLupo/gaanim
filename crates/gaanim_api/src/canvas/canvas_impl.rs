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
use gaanim_objects::prelude::{GltfDocument, GltfLoadError, GltfSceneSelector, SvgLoadError};
use gaanim_objects::primitives::{
    DEFAULT_ARROW_BODY_WIDTH, DEFAULT_ARROW_HEAD_LENGTH, DEFAULT_ARROW_HEAD_WIDTH,
};
use gaanim_objects::primitives3d;
use gaanim_text::prelude::TextRole;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType, BoundsTarget};
use crate::canvas::SceneMarker;
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{
    CameraBindingSpec, CameraBindingWindowSpec, CanvasCameraBindingKind, CanvasEndpoint, CanvasRay,
    CanvasState, LocalSegmentStop, Op, PointRef, Segment, SharedCameraBindingSpec,
    SharedCanvasState,
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
    track: AudioTrack,
    state: SharedCanvasState,
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
#[derive(Debug, Clone)]
pub enum PlayItem {
    Animation(Anim),
    Audio(AudioClip),
    Video(VideoClip),
    VideoSegment(VideoSegment),
    Lottie(LottieClip),
}

/// A pure, nestable description of temporal composition.
#[derive(Debug, Clone)]
pub struct Composition {
    node: CompositionNode,
    delay: f64,
    default_duration: Option<f64>,
    default_rate: Option<RateFunc>,
    stretch: Option<f64>,
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
}

impl Composition {
    pub fn leaf(item: impl Into<PlayItem>) -> Self {
        Self {
            node: CompositionNode::Leaf(Box::new(item.into())),
            delay: 0.0,
            default_duration: None,
            default_rate: None,
            stretch: None,
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
                }
                let (start, item_duration) = match &mut item {
                    PlayItem::Animation(anim) => {
                        let start = anim.inner.delay.max(0.0);
                        anim.inner.delay = 0.0;
                        (start, Some(anim.inner.duration.max(0.0)))
                    }
                    PlayItem::Audio(audio) => (0.0, audio.track.duration),
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
                    copy.start += offset;
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
        self.items.iter_mut().for_each(|item| item.start += offset);
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
    #[error("play contains multiple animations for target {target:?} channel '{channel}'")]
    ConflictingChannel {
        target: gaanim_core::ObjectId,
        channel: String,
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
    #[error("stretch() cannot contain Audio, Video, or Lottie leaves")]
    StretchContainsMedia,
    #[error("a zero-span composition can only be stretched to zero seconds")]
    CannotStretchZeroSpan,
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
        GrowFromPoint { .. } | GrowFromEdge { .. } => &["translation", "scale"],
        SpinInFromNothing => &["scale", "rotation"],
        Create3D => &["scale", "opacity"],
        Indicate { .. } => &["scale", "fill"],
        Transform { .. } | ReplacementTransform { .. } => &[
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
        | Wiggle => "translation",
        RotateTo { .. } | RotateBy { .. } | RotateBy3D { .. } => "rotation",
        ScaleTo { .. }
        | ScaleUniform { .. }
        | ScaleBy3D { .. }
        | GrowFromCenter
        | ShrinkToCenter => "scale",
        FadeTo { .. } | FadeIn | FadeOut | FadeInFrom { .. } => "opacity",
        FillColorTo { .. } | FillPaintTo { .. } => "fill",
        StrokeColorTo { .. } | StrokePaintTo { .. } => "stroke_color",
        StrokeWidthTo { .. } => "stroke_width",
        Material3DTo { .. } => "material",
        MediaFrameTo { .. } => "media_frame",
        SignalFloat { .. } => "signal",
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
        | CameraState { .. }
        | CameraReset => "camera_pose",
        CameraZoom { .. }
        | CameraZoomSource { .. }
        | CameraOrthographic { .. }
        | CameraPerspective { .. } => "camera_projection",
        CameraRotation { .. } | CameraRotationSource { .. } => "camera_rotation",
        CameraShake { .. } => "camera_shake",
        _ => "effect",
    };
    vec![channel.to_owned()]
}

/// Default length in scene units for the straight segments at either spring end.
pub const DEFAULT_SPRING_STRAIGHT: f64 = 12.0;

/// Default nominal font size for labels, values, and units in reactive annotations.
pub const DEFAULT_REACTIVE_TEXT_SIZE: f64 = 0.48;

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
}

impl std::fmt::Debug for CameraConstraintHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("CameraConstraintHandle").finish()
    }
}

impl CameraConstraintHandle {
    fn current_time(&self) -> f64 {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .iter()
            .map(|segment| segment.cursor)
            .sum()
    }

    /// Enable this constraint at the current timeline cursor.
    pub fn enable(&self) {
        let time = self.current_time();
        let mut spec = self.spec.lock().expect("camera binding poisoned");
        if spec
            .windows
            .last()
            .is_some_and(|window| window.end.is_none())
        {
            return;
        }
        spec.windows.push(CameraBindingWindowSpec {
            start: time,
            end: None,
        });
    }

    /// Disable this constraint at the current timeline cursor.
    pub fn disable(&self) {
        let time = self.current_time();
        let mut spec = self.spec.lock().expect("camera binding poisoned");
        if let Some(window) = spec.windows.last_mut()
            && window.end.is_none()
        {
            window.end = Some(time.max(window.start));
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
        #[source]
        source: ImageLoadError,
    },
    #[error("could not preload SVG '{path}': {source}")]
    Svg {
        path: PathBuf,
        #[source]
        source: SvgLoadError,
    },
    #[error("could not preload glTF '{path}': {source}")]
    Gltf {
        path: PathBuf,
        #[source]
        source: GltfLoadError,
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
static IMAGE_CACHE: OnceLock<Mutex<HashMap<PathBuf, gaanim_core::peniko::ImageData>>> =
    OnceLock::new();

/// Drop every process-local cache of files read by scenes (raster images,
/// Lottie, glTF metadata, and Typst layouts) so the next compile reads them
/// from disk again.
pub fn clear_asset_caches() {
    if let Some(cache) = IMAGE_CACHE.get() {
        cache.lock().expect("image cache poisoned").clear();
    }
    gaanim_objects::prelude::clear_gltf_cache();
    gaanim_renderer::lottie::clear_lottie_cache();
    gaanim_text::typst_compiler::clear_typst_layout_cache();
}

fn load_image(path: impl AsRef<Path>) -> Result<gaanim_core::peniko::ImageData, ImageLoadError> {
    let requested = path.as_ref();
    let cache_key = requested
        .canonicalize()
        .unwrap_or_else(|_| requested.to_path_buf());
    let cache = IMAGE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(image) = cache
        .lock()
        .expect("image cache poisoned")
        .get(&cache_key)
        .cloned()
    {
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
    Ok(cache.entry(cache_key).or_insert(image).clone())
}

/// Default height of a presentation brand logo in scene units.
const BRAND_LOGO_HEIGHT: f64 = 0.6;

/// Top-level facade for building Gaanim animations.
#[derive(Debug, Clone)]
pub struct SceneModel {
    pub frame: SceneFrame,
    pub background: Option<Color>,
    /// Full scene-bounds paint. `background` remains the representative color used
    /// for theme contrast and native 3D clears.
    pub background_paint: Option<gaanim_renderer::background::BackgroundPaint>,
    pub(crate) background_overridden: bool,
    /// WGSL post-processing applied to the rendered 2D scene inside the camera frame.
    pub post_process: Option<gaanim_renderer::post_process::PostProcessShader>,
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
    pub asset_root: Option<PathBuf>,
    /// Audio sources synchronized in preview and mixed by FFmpeg during export.
    pub audio_tracks: Vec<AudioTrack>,
    /// Voiceover blocks and the live take, for timing and the editor recorder.
    pub(crate) narration: super::narration::NarrationState,
    /// Reusable logo/footer treatment generated for every explicit segment.
    pub branding: Option<PresentationBrand>,
    pub(crate) camera_position: gaanim_core::glam::DVec3,
    pub(crate) lighting_3d: gaanim_scene::Lighting3D,
    pub(crate) state: SharedCanvasState,
}

impl SceneModel {
    pub fn new(width: impl Into<f64>, height: impl Into<f64>) -> Self {
        let width = width.into();
        let height = height.into();
        Self {
            frame: SceneFrame::new(width, height),
            background: None,
            background_paint: None,
            background_overridden: false,
            post_process: None,
            theme: None,
            theme_style: None,
            font_family_override: None,
            math_font_family_override: None,
            code_font_family_override: None,
            margin: Margin::default(),
            asset_root: None,
            audio_tracks: Vec::new(),
            narration: Default::default(),
            branding: None,
            camera_position: gaanim_core::glam::DVec3::ZERO,
            lighting_3d: gaanim_scene::Lighting3D::default(),
            state: Arc::new(Mutex::new(CanvasState::new())),
        }
    }

    /// Whether a handle was created by this exact Scene, independent of its
    /// numeric object ID.
    pub fn owns_drawable(&self, drawable: &DrawableHandle) -> bool {
        Arc::ptr_eq(&self.state, &drawable.state)
    }

    /// Whether replay requires Bevy's native 3D render graph.
    pub fn has_native_3d_content(&self) -> bool {
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
                            SpawnKind::GltfModel { .. }
                                | SpawnKind::Axes3D { .. }
                                | SpawnKind::Primitive3D(..)
                                | SpawnKind::SurfaceMesh { .. }
                                | SpawnKind::Polyline3D { .. }
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

    /// Replace or remove the scene post-process.
    pub fn set_post_process(
        &mut self,
        shader: Option<gaanim_renderer::post_process::PostProcessShader>,
    ) {
        self.post_process = shader;
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
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let target = guard
            .segments
            .iter_mut()
            .find(|candidate| candidate.id == segment.id)
            .ok_or(SegmentError::UnknownSegment { id: segment.id })?;
        target.post_process = post;
        Ok(())
    }

    /// Apply one of the built-in visual themes.
    ///
    /// `technical` is the quiet dark style used by the built-in technical
    /// components. `presentation` adds a warmer, higher-contrast hierarchy for
    /// projected slides. `paper` provides a light documentation canvas.
    /// Calling this method also selects the theme background; callers can
    /// still override [`SceneModel::background`] afterwards.
    pub fn set_theme(&mut self, name: &str) -> Result<(), ThemeError> {
        self.apply_theme(CanvasTheme::builtin(name)?);
        Ok(())
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

    /// Resolve a semantic token from the active theme.
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
        let mut config = self
            .theme_style
            .as_ref()
            .map(|theme| theme.text.clone())
            .unwrap_or_default();
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

        let source = crate::canvas::compile::structured_text_typst_source(
            spec,
            None,
            font_size,
            &font_family,
            color,
        );

        // Typst measurement reads only registered fonts; skip the system scan.
        let mut registry = gaanim_text::font::FontRegistry::without_system_fonts();
        self.register_theme_fonts(&mut registry);

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

    fn resolve_asset_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.asset_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
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
        })
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
            } else if extension.eq_ignore_ascii_case("gltf")
                || extension.eq_ignore_ascii_case("glb")
            {
                GltfDocument::load(&resolved, &GltfSceneSelector::Default).map_err(|source| {
                    AssetPreloadError::Gltf {
                        path: resolved.clone(),
                        source,
                    }
                })?;
            } else {
                load_image(&resolved).map_err(|source| AssetPreloadError::Image {
                    path: resolved.clone(),
                    source,
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
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let id = guard.next_object_id();
        let active_idx = guard.active_idx;
        if register_top_level {
            guard.active_mut().mobject_ids.push(id);
            guard.all_drawables.push(id);
        }
        drop(guard);

        let handle = DrawableHandle::new(id, kind, self.state.clone(), active_idx);
        {
            let mut state = self.state.lock().expect("canvas state poisoned");
            state.object_specs.insert(id, handle.spec.clone());
            state.segments[active_idx]
                .ops
                .push(Op::Spawn(handle.spec.clone()));
        }
        handle
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
        let mut segment = Segment::new(id, name, notes, template.clone(), background);
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
        let segment_number = guard
            .segments
            .iter()
            .filter(|segment| segment.explicit)
            .count();
        drop(guard);

        self.spawn_segment_branding(template.as_deref(), segment_number)?;
        Ok(SegmentHandle::new(id, self.state.clone()))
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
        let mut guard = self.state.lock().expect("canvas state poisoned");
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
        guard.segments[to_index].transition = Some(transition);
        guard.segments[to_index].prev_segment = Some(from_index);
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
        let drawable = self.spawn(SpawnKind::SurroundingRect).no_fill();
        let id = drawable.id;
        self.state
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
                    | SpawnKind::Polyline(_)
                    | SpawnKind::Bezier { .. }
                    | SpawnKind::Curve(_)
                    | SpawnKind::Image { .. }
                    | SpawnKind::Video { .. }
                    | SpawnKind::Lottie { .. }
                    | SpawnKind::Primitive3D(_)
                    | SpawnKind::Polyline3D { .. }
                    | SpawnKind::LineSegments3D { .. }
                    | SpawnKind::GltfNode { .. }
                    | SpawnKind::GltfModel { .. }
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
        drop(target);
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
    pub fn line_between(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        self.endpoint_line(from, to, false)
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
    pub fn double_arrow(
        &mut self,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        head_length: Option<f64>,
        head_width: Option<f64>,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::DoubleArrow {
            start: (x1, y1),
            end: (x2, y2),
            head_length,
            head_width,
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
            radius,
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
        self.spawn(SpawnKind::Polyline(points.to_vec()))
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

    /// Creates 3D Cartesian axes with three grid planes and perspective support.
    pub fn axes_3d(
        &mut self,
        x_range: (f64, f64, f64),
        y_range: (f64, f64, f64),
        z_range: (f64, f64, f64),
        config: crate::canvas::types::Axes3DConfig,
    ) -> DrawableHandle {
        self.spawn(SpawnKind::Axes3D {
            x_range,
            y_range,
            z_range,
            config,
        })
    }

    /// Creates a triangulated 3D surface mesh in world space.
    /// Vertices are expected in world coordinates (use `axes_3d` scale to compute).
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
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cylinder(
            radius, height, segments, caps, material,
        )?)))
    }

    pub fn cone(
        &mut self,
        radius: f64,
        height: f64,
        segments: u32,
        cap: bool,
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::cone(
            radius, height, segments, cap, material,
        )?)))
    }

    pub fn plane(
        &mut self,
        width: f64,
        height: f64,
        subdivisions: (u32, u32),
        material: gaanim_scene::Material3D,
    ) -> Result<DrawableHandle, primitives3d::Primitive3DError> {
        Ok(self.spawn(SpawnKind::Primitive3D(primitives3d::plane(
            width,
            height,
            subdivisions,
            material,
        )?)))
    }

    pub fn lighting_3d(&mut self, enabled: bool, intensity: f32, shadows: bool) {
        self.lighting_3d = gaanim_scene::Lighting3D {
            enabled,
            intensity,
            shadows,
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
                handle = handle.color_by(part.text.clone(), color);
            }
            handle = handle.define_tag(part.path.join("."), part.text, Some(part.occurrence));
        }
        handle
    }
    /// Compile full Typst markup, including tables and other document layout.
    pub fn typst(&mut self, source: &str) -> DrawableHandle {
        self.typst_inner(source, None)
    }

    /// Compile full Typst markup with a custom page width (e.g. `"16cm"`, `"800pt"`, `"12in"`).
    pub fn typst_with_width(&mut self, source: &str, page_width: &str) -> DrawableHandle {
        self.typst_inner(source, Some(page_width))
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
        let source = std::fs::read_to_string(&path)
            .map_err(|source| TypstAssetError::Read { path, source })?;
        Ok(self.typst_inner(&source, page_width))
    }

    fn typst_inner(&mut self, source: &str, page_width: Option<&str>) -> DrawableHandle {
        self.spawn(SpawnKind::Typst {
            source: source.to_string(),
            page_width: page_width.map(|w| w.to_string()),
            scene_units: false,
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
        self.transform_matching(source, target, "shapes", duration)
    }

    /// Generic auto-matching morph. `mode` can be `"shapes"` or `"tex"`.
    ///
    /// Queues an `Op::TransformMatching` operation to pair and morph submobjects between
    /// `source` and `target`.
    pub fn transform_matching(
        &mut self,
        source: &DrawableHandle,
        target: &DrawableHandle,
        mode: &str,
        duration: f64,
    ) -> DrawableHandle {
        if !Arc::ptr_eq(&self.state, &source.state)
            || !Arc::ptr_eq(&self.state, &target.state)
            || !duration.is_finite()
            || duration <= 0.0
        {
            return target.clone();
        }
        let mode = match mode.to_ascii_lowercase().as_str() {
            "tex" | "text" | "chars" => "tex".to_string(),
            _ => "shapes".to_string(),
        };
        let semantic_pairs = if mode == "tex" {
            Self::text_semantic_pairs(source, target, None)
        } else {
            Vec::new()
        };
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::TransformMatching {
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

    /// Load an image with an optional target size, fit mode, and source crop.
    pub fn image_with_options(
        &mut self,
        path: impl AsRef<Path>,
        mut options: ImageOptions,
    ) -> Result<super::ImageHandle, ImageLoadError> {
        let image = load_image(self.resolve_asset_path(path))?;
        if options.width.is_none() && options.height.is_none() {
            let safe = self.safe_frame();
            options.width = Some(safe.width());
            options.height = Some(safe.height());
        }
        let view = options.resolve(image.width, image.height)?;
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
        mut options: VideoOptions,
    ) -> Result<VideoClip, VideoLoadError> {
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

        let path = self.resolve_asset_path(path);
        let metadata = gaanim_media::probe_video(&path)?;
        if options.offset >= metadata.duration {
            return Err(VideoLoadError::OffsetOutOfRange);
        }
        let source_duration = options
            .duration
            .unwrap_or(metadata.duration - options.offset);
        if options.offset + source_duration > metadata.duration + 1e-6 {
            return Err(VideoLoadError::DurationOutOfRange);
        }
        if options.image.width.is_none() && options.image.height.is_none() {
            let safe = self.safe_frame();
            options.image.width = Some(safe.width());
            options.image.height = Some(safe.height());
        }
        let view = options.image.resolve(metadata.width, metadata.height)?;
        let poster = gaanim_media::decode_video_frame(&path, &metadata, options.offset)?;
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

    /// Load an SVG as an animatable group of resolved vector paths.
    ///
    /// Shapes, paths, solid or gradient paints, outlined text, transforms,
    /// CSS, `<use>`, `viewBox`, and vector `clipPath` groups are imported.
    /// Raster images, patterns, masks, and arbitrary filters remain omitted.
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

    /// Import the default scene of a local glTF 2.0 `.gltf` or `.glb` model.
    pub fn gltf(&mut self, path: impl AsRef<Path>) -> Result<DrawableHandle, GltfLoadError> {
        self.gltf_scene(path, GltfSceneSelector::Default)
    }

    /// Import a selected glTF scene by name or index.
    pub fn gltf_scene(
        &mut self,
        path: impl AsRef<Path>,
        selector: GltfSceneSelector,
    ) -> Result<DrawableHandle, GltfLoadError> {
        let document = GltfDocument::load(self.resolve_asset_path(path), &selector)?;
        let mut handles = HashMap::<usize, DrawableHandle>::new();
        for node in &document.nodes {
            let handle = self.spawn_registered(
                SpawnKind::GltfNode {
                    node_index: node.index,
                    path: node.path.clone(),
                    bounds: node.bounds,
                },
                false,
            );
            handles.insert(node.index, handle);
        }

        let bindings = document
            .nodes
            .iter()
            .map(|node| {
                (
                    node.index,
                    node.parent,
                    node.path.clone(),
                    handles[&node.index].id,
                )
            })
            .collect();
        let animation_names = document
            .animations
            .iter()
            .map(|animation| animation.name.clone())
            .collect::<Vec<_>>();
        let root = self.spawn(SpawnKind::GltfModel {
            path: document.path,
            scene_index: document.scene_index,
            bounds: document.bounds,
            nodes: bindings,
            animation_names: animation_names.clone(),
        });

        let mut parts = HashMap::new();
        let mut short_counts = HashMap::<String, usize>::new();
        for node in &document.nodes {
            *short_counts.entry(node.name.clone()).or_default() += 1;
        }
        for node in &document.nodes {
            let handle = handles[&node.index].clone();
            parts.insert(node.path.clone(), handle.clone());
            if short_counts.get(&node.name) == Some(&1) {
                parts.insert(node.name.clone(), handle);
            }
        }
        Ok(root.with_gltf_metadata(parts, document.animations))
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
                    if !path.id.is_empty() {
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
            parts.insert(group.id.clone(), handle.clone());
        }
        (handle, leaf_specs)
    }

    pub fn group(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        let handle = self
            .spawn(SpawnKind::Group(members.iter().map(|m| m.id).collect()))
            .with_style_targets(
                members
                    .iter()
                    .flat_map(|member| member.inherited_style_targets())
                    .collect(),
            );
        handle
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

    /// Queue a layout recalculation. `duration = Some(_)` animates the move
    /// and fades the newly inserted member in; `None` updates immediately.
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

    fn camera_anim(&self, ty: AnimationType, duration: f64) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(
            gaanim_core::ObjectId::from_raw(u64::MAX),
            ty,
            self.state.clone(),
            active_idx,
        )
        .duration(duration.max(0.0))
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
        let pose = gaanim_math::CameraPose::perspective_3d(eye, target, up, fov_y, near, far)?;
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
    pub fn camera_to(
        &self,
        state_handle: &CameraStateHandle,
        duration: f64,
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
        self.camera_anim(
            AnimationType::CameraFollowEndpoint {
                target,
                offset,
                offset_space,
                lag,
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
        self.camera_anim(
            AnimationType::CameraShake {
                amplitude: shake.amplitude,
                frequency: shake.frequency,
                trauma: Some(shake),
            },
            duration,
        )
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
        let up = up.map(|(x, y, z)| DVec3::new(x, y, z)).unwrap_or(DVec3::Y);
        self.camera_anim(AnimationType::CameraLookAt { eye, target, up }, duration)
    }

    /// Aim the camera using native reactive endpoints.
    pub fn camera_look_at_endpoints(
        &mut self,
        eye: CanvasEndpoint,
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
        self.camera_anim(
            AnimationType::CameraOrbit {
                delta_yaw,
                delta_pitch,
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

    fn camera_binding(
        &mut self,
        kind: CanvasCameraBindingKind,
        influence: ScalarSource,
        enabled: bool,
    ) -> CameraConstraintHandle {
        let mut state = self.state.lock().expect("canvas state poisoned");
        let start = state.segments.iter().map(|segment| segment.cursor).sum();
        let order = state.next_camera_binding_order();
        let spec = Arc::new(Mutex::new(CameraBindingSpec {
            order,
            kind,
            influence,
            windows: enabled
                .then_some(CameraBindingWindowSpec { start, end: None })
                .into_iter()
                .collect(),
        }));
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
    pub fn camera_bind_3d(
        &mut self,
        eye: Option<CanvasEndpoint>,
        target: Option<CanvasEndpoint>,
        fov_y: Option<ScalarSource>,
        up: DVec3,
        influence: ScalarSource,
        enabled: bool,
    ) -> Result<CameraConstraintHandle, CameraBindingError> {
        if eye.is_none() && target.is_none() && fov_y.is_none() {
            return Err(CameraBindingError::Empty);
        }
        if !up.is_finite() || up.length_squared() <= f64::EPSILON {
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
        let mut occupied: Vec<(gaanim_core::ObjectId, String, f64, f64)> = Vec::new();
        for resolved_item in &resolved {
            let PlayItem::Animation(anim) = &resolved_item.item else {
                continue;
            };
            let start = resolved_item.start;
            let end = start + resolved_item.duration.unwrap_or(0.0);
            for channel in animation_channels(anim) {
                if self
                    .state
                    .lock()
                    .expect("canvas state poisoned")
                    .bound_properties
                    .iter()
                    .any(|(target, bound)| {
                        *target == anim.inner.target
                            && (bound.name() == channel || channel == "effect")
                    })
                {
                    return Err(PlayError::BoundProperty {
                        target: anim.inner.target,
                        channel,
                    });
                }
                let conflicts = occupied.iter().any(
                    |(target, occupied_channel, occupied_start, occupied_end)| {
                        if *target != anim.inner.target || *occupied_channel != channel {
                            return false;
                        }
                        let left_zero = end == start;
                        let right_zero = occupied_end == occupied_start;
                        match (left_zero, right_zero) {
                            (true, true) => start == *occupied_start,
                            (true, false) => start >= *occupied_start && start < *occupied_end,
                            (false, true) => *occupied_start >= start && *occupied_start < end,
                            (false, false) => start < *occupied_end && *occupied_start < end,
                        }
                    },
                );
                if conflicts {
                    return Err(PlayError::ConflictingChannel {
                        target: anim.inner.target,
                        channel,
                    });
                }
                occupied.push((anim.inner.target, channel, start, end));
            }
        }
        if resolved.iter().any(
            |item| matches!(&item.item, PlayItem::Audio(audio) if !audio.belongs_to(&self.state)),
        ) {
            return Err(PlayError::ForeignAudio);
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

        let play_start = self.current_time();
        let mut builders = Vec::new();
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
                    builder.delay += delay;
                    visual_duration =
                        visual_duration.max(builder.delay.max(0.0) + builder.duration.max(0.0));
                    builders.push(builder);
                }
                PlayItem::Audio(audio) => {
                    let mut track = audio.track;
                    track.start_time = play_start + delay;
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

        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += max_duration;
        for id in camera_captures {
            guard.active_mut().ops.push(Op::CaptureCameraState { id });
        }
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
        if matches!(template, Some("title_slide" | "title" | "cover")) && !branding.show_on_cover {
            return Ok(());
        }

        let frame = self.safe_frame();
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
            self.line(frame.min.x, rule_y, frame.max.x, rule_y)
                .no_fill()
                .stroke(rule, 0.02)
                .z_index(100);
        }
        let footer = match (branding.footer.as_deref(), branding.slide_numbers) {
            (Some(footer), true) => Some(format!("{footer}    ·    {segment_number:02}")),
            (Some(footer), false) => Some(footer.to_owned()),
            (None, true) => Some(format!("{segment_number:02}")),
            (None, false) => None,
        };
        if let Some(footer) = footer {
            self.text(&footer)
                .fill(muted)
                .scale_to(0.5)
                .move_to(frame.min.x + frame.width() * 0.14, footer_y + 0.08)
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
    /// shared boundary until playback advances into the next segment. After a
    /// recorded live take started, the stop becomes the pause recorded there.
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
        if let Some(hold) = self.live_take_hold() {
            self.wait(hold);
            return Ok(());
        }
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
        segment.stops.push(LocalSegmentStop { name, time });
        segment.ops.push(Op::Stop);
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
        let mut state = self.state.lock().expect("canvas state poisoned");
        let segment = state.active_mut();
        let time = segment.cursor;
        segment.markers.push((name, time));
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
                    notes: segment.notes.clone(),
                    template: segment.template.clone(),
                    start_time,
                    end_time,
                    stops: segment
                        .stops
                        .iter()
                        .map(|stop| SegmentStop {
                            name: stop.name.clone(),
                            time: start_time + stop.time,
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
        tracker: &DrawableHandle,
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
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden line centered and aligned with the tangent of `curve`.
    pub fn tangent_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
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
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden line centered and perpendicular to the tangent of `curve`.
    pub fn normal_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
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
                tracker: tracker.id,
            });
        handle
    }

    /// Spawn a hidden unit circle scaled to the local osculating circle of `curve`.
    pub fn curvature_on_curve(
        &mut self,
        curve: &DrawableHandle,
        tracker: &DrawableHandle,
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
                tracker: tracker.id,
                window,
            });
        handle
    }

    /// Creates a hidden curved arrow whose sweep is regenerated from `tracker`
    /// on every frame. The effective sweep is `value * sweep_scale + sweep_offset`.
    pub fn always_redraw_arc(
        &mut self,
        tracker: &DrawableHandle,
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
                tracker: tracker.id,
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
        PointRef(CanvasEndpoint::Expression { x, y })
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
        alpha: f64,
        offset: DVec3,
    ) -> PointRef {
        PointRef(CanvasEndpoint::Between {
            from: Box::new(from),
            to: Box::new(to),
            alpha,
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
        let mut style = gaanim_text::prelude::TextStyle::default();
        style.size = Some(font_size.unwrap_or(DEFAULT_REACTIVE_TEXT_SIZE));
        style.color = color;
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
        tracker: &DrawableHandle,
        tangent_length: f64,
        normal_length: f64,
    ) -> DrawableHandle {
        let point = self.point_on_curve(curve, tracker);
        let tangent = self.tangent_on_curve(curve, tracker, tangent_length);
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
            alpha: 0.0,
            offset: DVec3::new(x.x * length, x.y * length, 0.0),
        });
        let y_tip = PointRef(CanvasEndpoint::Between {
            from: Box::new(origin.clone()),
            to: Box::new(origin.clone()),
            alpha: 0.0,
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
            fn valid(point: &CanvasEndpoint, state: &CanvasState) -> bool {
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
                    CanvasEndpoint::Expression { x, y } => scalar(x) && scalar(y),
                    CanvasEndpoint::LocalExpression { space, x, y, z } => {
                        state.object_specs.contains_key(space)
                            && scalar(x)
                            && scalar(y)
                            && scalar(z)
                    }
                    CanvasEndpoint::LocalNumberLine {
                        space,
                        value,
                        normal_offset,
                        ..
                    } => {
                        state.object_specs.contains_key(space)
                            && scalar(value)
                            && scalar(normal_offset)
                    }
                    CanvasEndpoint::Offset { origin, dx, dy } => {
                        valid(origin, state) && scalar(dx) && scalar(dy)
                    }
                    CanvasEndpoint::Between {
                        from,
                        to,
                        alpha,
                        offset,
                    } => {
                        valid(from, state)
                            && valid(to, state)
                            && alpha.is_finite()
                            && offset.is_finite()
                    }
                    CanvasEndpoint::Polar {
                        origin,
                        radius,
                        angle,
                    } => valid(origin, state) && scalar(radius) && scalar(angle),
                }
            }
            if !points.iter().all(|p| valid(p, &state)) {
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

    fn endpoint_line(
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
        let line = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        let extensions = self.spawn(SpawnKind::TrackingLine).fill(color).no_stroke();
        line.defer_visibility_until_play();
        extensions.defer_visibility_until_play();
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
        let mut scene = self.clone();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimension_label_style_reaches_label_value_and_unit() {
        let mut canvas = SceneModel::new(16, 9);
        let style = gaanim_text::prelude::TextStyle {
            font: Some("Cascadia Mono".into()),
            weight: Some(700),
            color: Some(Color::from_rgb8(200, 30, 30)),
            ..Default::default()
        };
        let dimension = canvas
            .dimension_between_with_options(
                CanvasEndpoint::Static(DVec3::ZERO),
                CanvasEndpoint::Static(DVec3::new(0.0, 2.0, 0.0)),
                0.5,
                DimensionOptions {
                    label: Some("h".into()),
                    show_value: true,
                    unit: Some("m".into()),
                    side: Some(gaanim_animation::DimensionSide::Right),
                    font_size: Some(0.4),
                    label_style: style,
                    color: Some(Color::BLACK),
                    ..Default::default()
                },
            )
            .unwrap();
        for part in [&dimension.label, &dimension.unit] {
            let spec = part.as_ref().unwrap().spec.lock().unwrap();
            let SpawnKind::Text(text) = &spec.kind else {
                panic!("expected a text part");
            };
            assert_eq!(text.style.font.as_deref(), Some("Cascadia Mono"));
            assert_eq!(text.style.weight, Some(700));
            assert_eq!(text.style.size, Some(0.4));
            assert_eq!(text.style.color, Some(Color::from_rgb8(200, 30, 30)));
        }
        let number = dimension.number.unwrap();
        let spec = number.spec.lock().unwrap();
        let SpawnKind::ReactiveReadout {
            font_family,
            font_weight,
            font_size,
            ..
        } = &spec.kind
        else {
            panic!("expected a reactive readout");
        };
        assert_eq!(font_family.as_deref(), Some("Cascadia Mono"));
        assert_eq!(*font_weight, Some(700));
        assert_eq!(*font_size, Some(0.4));
        drop(spec);
        let state = canvas.state.lock().unwrap();
        let sides: Vec<_> = state
            .active()
            .ops
            .iter()
            .filter_map(|op| match op {
                Op::AttachTrackingDimension { side, .. }
                | Op::AttachDimensionLabelPlacement { side, .. } => Some(*side),
                _ => None,
            })
            .collect();
        assert_eq!(sides, [Some(gaanim_animation::DimensionSide::Right); 2]);
    }

    #[test]
    fn dimensioned_arrow_caps_head_and_rejects_invalid_inputs() {
        let mut canvas = SceneModel::new(16, 9);
        let arrow = canvas
            .arrow_with_dimensions((0.0, 0.0), (0.2, 0.0), 0.18, 0.15, 0.036, Some(0.3))
            .unwrap();
        let spec = arrow.spec.lock().unwrap();
        let SpawnKind::SizedArrow {
            head_length,
            head_width,
            body_width,
            ..
        } = spec.kind
        else {
            panic!("expected dimensioned arrow");
        };
        assert!((head_length - 0.06).abs() < 1e-12);
        assert!((head_width - 0.05).abs() < 1e-12);
        assert_eq!(body_width, 0.036);
        drop(spec);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                canvas
                    .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), bad, 0.15, 0.036, None)
                    .is_err()
            );
            assert!(
                canvas
                    .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, bad, 0.036, None)
                    .is_err()
            );
            assert!(
                canvas
                    .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, bad, None)
                    .is_err()
            );
            assert!(
                canvas
                    .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, Some(bad))
                    .is_err()
            );
        }
        assert!(
            canvas
                .arrow_with_dimensions((f64::NAN, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, None)
                .is_err()
        );
        assert!(
            canvas
                .arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.18, 0.15, 0.036, Some(1.1))
                .is_err()
        );
    }

    #[test]
    fn curved_arrows_accept_arrow_dimensions_capped_by_arc_length() {
        let mut canvas = SceneModel::new(16, 9);
        let dimensions = |handle: &DrawableHandle| match handle.spec.lock().unwrap().kind {
            SpawnKind::CurvedArrow {
                head_length,
                head_width,
                body_width,
                ..
            }
            | SpawnKind::CurvedArrowArc {
                head_length,
                head_width,
                body_width,
                ..
            } => (head_length, head_width, body_width),
            _ => panic!("expected a curved arrow"),
        };
        let default = canvas.curved_arrow(-1.0, 0.0, 1.0, 0.0, 0.9);
        assert_eq!(
            dimensions(&default),
            (
                DEFAULT_ARROW_HEAD_LENGTH,
                DEFAULT_ARROW_HEAD_WIDTH,
                DEFAULT_ARROW_BODY_WIDTH
            )
        );
        let bold = canvas
            .curved_arrow_with_dimensions((-1.0, 0.0), (1.0, 0.0), 0.9, 0.3, 0.24, 0.06, None)
            .unwrap();
        assert_eq!(dimensions(&bold), (0.3, 0.24, 0.06));
        // A quarter turn of radius 0.4 is about 0.63 long; 30 % of it is 0.19.
        let arc_length = 0.4 * std::f64::consts::FRAC_PI_2;
        let capped = canvas
            .curved_arrow_arc_with_dimensions(
                (0.0, 0.0),
                0.4,
                0.0,
                std::f64::consts::FRAC_PI_2,
                0.4,
                0.2,
                0.05,
                Some(0.3),
            )
            .unwrap();
        let (head_length, head_width, body_width) = dimensions(&capped);
        assert!((head_length - arc_length * 0.3).abs() < 1e-12);
        assert!((head_width - 0.2 * arc_length * 0.3 / 0.4).abs() < 1e-12);
        assert_eq!(body_width, 0.05);

        assert!(
            canvas
                .curved_arrow_with_dimensions((0.0, 0.0), (1.0, 0.0), 0.5, 0.0, 0.2, 0.05, None)
                .is_err()
        );
        assert!(
            canvas
                .curved_arrow_with_dimensions(
                    (0.0, 0.0),
                    (1.0, 0.0),
                    f64::NAN,
                    0.3,
                    0.2,
                    0.05,
                    None
                )
                .is_err()
        );
        assert!(
            canvas
                .curved_arrow_arc_with_dimensions(
                    (0.0, 0.0),
                    1.0,
                    0.0,
                    1.0,
                    0.3,
                    0.2,
                    0.05,
                    Some(0.0)
                )
                .is_err()
        );
    }

    #[test]
    fn typst_asset_uses_the_configured_asset_root() {
        let root = std::env::temp_dir().join(format!(
            "gaanim_typst_asset_canvas_test_{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("title.typ"), "= Asset-backed Typst").unwrap();

        let mut canvas = SceneModel::new(640, 360);
        canvas.set_asset_root(&root).unwrap();
        let handle = canvas.typst_asset("title.typ").unwrap();
        let spec = handle.spec.lock().unwrap();
        let SpawnKind::Typst { source, .. } = &spec.kind else {
            panic!("typst asset should create a Typst drawable");
        };
        assert_eq!(source, "= Asset-backed Typst");
    }

    #[test]
    fn surrounding_rect_validates_geometry_and_tracks_retarget_cursor() {
        let mut canvas = SceneModel::new(640, 360);
        let left = canvas.circle(20.0);
        let right = canvas.rect(80.0, 30.0);
        assert!(matches!(
            canvas.surrounding_rect(vec![], [12.0; 4], 8.0),
            Err(SurroundingRectError::NoTargets)
        ));
        let frame = canvas
            .surrounding_rect(vec![left.bounds_target()], [8.0, 12.0, 8.0, 12.0], 6.0)
            .unwrap();
        let animation = frame
            .retarget(vec![right.bounds_target()], Some(0.75))
            .unwrap();
        assert_eq!(animation.inner.duration, 0.75);
        assert_eq!(*frame.targets.lock().unwrap(), vec![right.bounds_target()]);
        assert!(frame.drawable.is_live_derived_geometry());
    }
    use crate::canvas::ops::Op;
    use bevy::prelude::World;
    use gaanim_math::SpatialTransform;
    use gaanim_scene::{MobjectId, Opacity};
    use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
    use gaanim_timeline::scene::SceneMember;
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    trait UnifiedTextFixture {
        fn math_text(&mut self, source: &str) -> DrawableHandle;
        fn test_title(&mut self, source: &str) -> DrawableHandle;
    }

    impl UnifiedTextFixture for SceneModel {
        fn math_text(&mut self, source: &str) -> DrawableHandle {
            self.text(&format!("${source}$"))
        }

        fn test_title(&mut self, source: &str) -> DrawableHandle {
            let spec = gaanim_text::prelude::TextSpec::new(
                vec![source.into()],
                Some(gaanim_text::prelude::TextRole::Title),
                gaanim_text::prelude::TextStyle::default(),
                gaanim_text::prelude::TextFlow::default(),
            )
            .expect("valid title fixture");
            self.text_spec(spec)
        }
    }

    fn compile_updater_count(canvas: &SceneModel) -> usize {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        world
            .query_filtered::<bevy::prelude::Entity, bevy::prelude::With<gaanim_animation::Updater>>()
            .iter(&world)
            .count()
    }

    #[test]
    fn step_equation_preserves_explicit_tag_mapping_and_occurrence() {
        let mut canvas = SceneModel::new(320, 180);
        let source = canvas
            .math_text("x + x = 2x")
            .define_tag("right_x", "x", Some(1));
        let target = canvas
            .math_text("x = y")
            .define_tag("renamed", "y", Some(0));
        let step = source
            .step_to(
                &target,
                Some(vec![("right_x".to_string(), "renamed".to_string())]),
                0.8,
            )
            .unwrap();
        canvas.play(vec![step]);

        let state = canvas.state.lock().expect("canvas state poisoned");
        let pairs = state
            .segments
            .iter()
            .flat_map(|segment| &segment.ops)
            .find_map(|op| match op {
                Op::Play(anims) => anims.iter().find_map(|anim| match &anim.anim_type {
                    AnimationType::TextTransition { semantic_pairs, .. } => Some(semantic_pairs),
                    _ => None,
                }),
                _ => None,
            })
            .expect("step equation op should be queued");
        assert_eq!(
            pairs,
            &vec![("x".to_string(), Some(1), "y".to_string(), Some(0),)]
        );
    }

    #[test]
    fn text_selection_compound_properties_seek_only_selected_glyphs() {
        let mut canvas = SceneModel::new(320, 180);
        let text = canvas.text("ABC");
        let selection = text.select("B");
        let red = Color::from_rgb8(255, 0, 0);
        canvas.play(vec![
            selection
                .animate_properties()
                .fill(red)
                .opacity(0.25)
                .duration(1.0),
        ]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
        let animated_targets = timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(animation)
                    if matches!(
                        animation.lens,
                        PropertyLensSpec::FillColor { .. } | PropertyLensSpec::Opacity { .. }
                    ) =>
                {
                    Some(animation.target)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(
            animated_targets.len(),
            2,
            "one selected glyph must receive two channels"
        );
        assert_eq!(animated_targets[0], animated_targets[1]);
        let entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == animated_targets[0]).then_some(entity))
            .expect("selected glyph entity");

        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.0);
        let start_opacity = world.get::<Opacity>(entity).expect("start opacity").0;
        timeline.seek(&mut world, 0.5);
        let mid_opacity = world.get::<Opacity>(entity).expect("mid opacity").0;
        timeline.seek(&mut world, 1.0);
        let end_opacity = world.get::<Opacity>(entity).expect("end opacity").0;
        assert!((start_opacity - 1.0).abs() < 1e-6);
        assert!(mid_opacity < start_opacity && mid_opacity > end_opacity);
        assert!((end_opacity - 0.25).abs() < 1e-6);
        assert!(matches!(
            world.get::<gaanim_scene::FillBrush>(entity).and_then(|fill| fill.0.as_ref()),
            Some(Brush::Solid(color)) if *color == red
        ));
    }

    #[test]
    fn custom_updater_survives_preview_and_export_recompilation() {
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.dot(8.0);
        dot.add_custom_updater(gaanim_animation::Updater::new(
            |_dt, _elapsed, _entity, _world| true,
        ));

        assert_eq!(compile_updater_count(&canvas), 1);
        assert_eq!(compile_updater_count(&canvas), 1);
    }

    #[test]
    fn reactive_objects_do_not_run_before_their_authored_cursor() {
        let mut canvas = SceneModel::new(320, 180);
        canvas.wait(2.0);
        let dot = canvas.dot(8.0);
        dot.add_custom_updater(gaanim_animation::Updater::new(
            |_dt, elapsed, entity, world| {
                if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                    transform.translation.x = elapsed;
                }
                true
            },
        ));
        let _trail = canvas.traced_path(&dot);
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 1.0);

        let mut updater_query = world.query_filtered::<
            (bevy::prelude::Entity, &SpatialTransform),
            bevy::prelude::With<gaanim_animation::Updater>,
        >();
        let (_, transform) = updater_query.single(&world).unwrap();
        assert_eq!(transform.translation.x, 0.0);

        let mut trail_query = world.query::<&gaanim_animation::TracedPath>();
        let traced_path = trail_query.single(&world).unwrap();
        assert!(traced_path.points.is_empty());
    }

    #[test]
    fn traced_path_requires_an_explicit_entry_animation() {
        let mut hidden_canvas = SceneModel::new(320, 180);
        let hidden_dot = hidden_canvas.dot(8.0);
        let _hidden_trail = hidden_canvas.traced_path(&hidden_dot);
        hidden_canvas.wait(1.0);

        let mut hidden_world = World::new();
        hidden_world.insert_resource(Timeline::new());
        hidden_world.insert_resource(gaanim_animation::PlaybackState::default());
        hidden_world.insert_resource(gaanim_text::font::FontRegistry::new());
        hidden_world.insert_resource(gaanim_text::prelude::TextConfig::default());
        hidden_canvas.compile(&mut hidden_world);
        hidden_world.flush();

        let hidden_opacity = hidden_world
            .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
            .single(&hidden_world)
            .unwrap();
        assert_eq!(hidden_opacity.0, 0.0);

        let mut animated_canvas = SceneModel::new(320, 180);
        let animated_dot = animated_canvas.dot(8.0);
        let animated_trail = animated_canvas.traced_path(&animated_dot);
        animated_canvas.play(vec![animated_trail.fade_in(1.0)]);

        let mut animated_world = World::new();
        animated_world.insert_resource(Timeline::new());
        animated_world.insert_resource(gaanim_animation::PlaybackState::default());
        animated_world.insert_resource(gaanim_text::font::FontRegistry::new());
        animated_world.insert_resource(gaanim_text::prelude::TextConfig::default());
        animated_canvas.compile(&mut animated_world);
        animated_world.flush();

        let snapshot = WorldSnapshot::capture(&mut animated_world);
        let mut timeline = animated_world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut animated_world, 0.5);
        let halfway = animated_world
            .query_filtered::<&Opacity, bevy::prelude::With<gaanim_animation::TracedPath>>()
            .single(&animated_world)
            .unwrap()
            .0;
        assert!(halfway > 0.0 && halfway < 1.0);
    }

    #[test]
    fn reactive_visuals_require_their_own_play_entry() {
        let mut canvas = SceneModel::new(320, 180);
        let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
        let mass = canvas.dot(8.0).move_to(60.0, 0.0);
        let spring = canvas.spring_between_with_crossing(
            CanvasEndpoint::Entity(anchor.id),
            CanvasEndpoint::Entity(mass.id),
            6,
            10.0,
            1.0,
        );
        let dimension = canvas.dimension_between(
            CanvasEndpoint::Entity(anchor.id),
            CanvasEndpoint::Entity(mass.id),
            -24.0,
        );
        let label = canvas.text("mass");
        label.follow_to(&mass, 0.0, 28.0);
        canvas.play(vec![
            spring.fade_in(1.0),
            dimension.create(1.0),
            label.fade_in(1.0),
        ]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            // SceneBuilder's id counter starts at zero while SceneModel ids start
            // at one; the test resolves the corresponding compiled root.
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
            query
                .iter(world)
                .find(|(object_id, _)| object_id.0 == compiled_id)
                .map(|(_, opacity)| opacity.0)
                .expect("reactive visual should compile into a mobject")
        };

        assert_eq!(opacity_for(&mut world, anchor.id), 1.0);
        assert_eq!(opacity_for(&mut world, spring.id), 0.0);
        assert_eq!(opacity_for(&mut world, dimension.id), 0.0);
        assert_eq!(opacity_for(&mut world, label.id), 0.0);

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);

        let spring_opacity = opacity_for(&mut world, spring.id);
        let dimension_opacity = opacity_for(&mut world, dimension.id);
        let label_opacity = opacity_for(&mut world, label.id);
        assert!(spring_opacity > 0.0);
        assert!(dimension_opacity > 0.0);
        assert!(label_opacity > 0.0);
    }

    #[test]
    fn length_lines_preserve_size_and_orientation_with_next_to() {
        use gaanim_layout::Direction;
        let mut canvas = SceneModel::new(640, 360);
        let card = canvas.rect(4.0, 2.0).move_to(1.0, 1.0);
        let horizontal = canvas
            .line_with_length(3.0, Direction::Right)
            .unwrap()
            .next_to(&card, Direction::Down, 0.5);
        let vertical = canvas
            .line_with_length(3.0, Direction::Up)
            .unwrap()
            .next_to(&card, Direction::Right, 0.5);
        let diagonal = canvas
            .line_with_length(5.0, Direction::Custom(DVec3::new(3.0, 4.0, 0.0)))
            .unwrap();
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        for (handle, expected_start, expected_end) in [
            (horizontal, (-0.5, -0.5), (2.5, -0.5)),
            (vertical, (3.5, -0.5), (3.5, 2.5)),
            (diagonal, (-1.5, -2.0), (1.5, 2.0)),
        ] {
            let id = gaanim_core::ObjectId::from_raw(handle.id.as_raw() - 1);
            let (_, path, transform) = world
                .query::<(
                    &gaanim_scene::MobjectId,
                    &gaanim_scene::Path2D,
                    &gaanim_math::SpatialTransform,
                )>()
                .iter(&world)
                .find(|(object, _, _)| object.0 == id)
                .unwrap();
            let affine = transform.to_affine_2d();
            let elements = path.0.elements();
            let [
                gaanim_core::kurbo::PathEl::MoveTo(start),
                gaanim_core::kurbo::PathEl::LineTo(end),
            ] = elements
            else {
                panic!("expected a single line segment");
            };
            assert!((affine * *start).distance(expected_start.into()) < 1e-9);
            assert!((affine * *end).distance(expected_end.into()) < 1e-9);
        }
    }

    #[test]
    fn length_lines_reject_invalid_geometry() {
        use gaanim_layout::Direction;
        let mut canvas = SceneModel::new(640, 360);
        for length in [0.0, -1.0, f64::INFINITY, f64::NAN] {
            assert!(canvas.line_with_length(length, Direction::Right).is_err());
        }
        for vector in [
            DVec3::ZERO,
            DVec3::Z,
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::splat(f64::NAN),
        ] {
            assert!(
                canvas
                    .line_with_length(2.0, Direction::Custom(vector))
                    .is_err()
            );
        }
    }

    #[test]
    fn line_between_accepts_static_and_anchor_endpoints() {
        let mut canvas = SceneModel::new(320, 180);
        let reference = canvas.rect(100.0, 40.0).move_to(30.0, 40.0);
        let anchor = reference.anchor_point(Anchor::TopRight, DVec3::ZERO);
        canvas.line_between(
            CanvasEndpoint::Static(DVec3::new(-10.0, -20.0, 0.0)),
            anchor.into(),
        );

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        gaanim_animation::tracking_line_system(&mut world);

        let path = world
            .query_filtered::<&gaanim_scene::PathSource, With<gaanim_animation::TrackingLine>>()
            .single(&world)
            .expect("one endpoint line");
        let elements = path.0.elements();
        assert!(matches!(
            elements.first(),
            Some(gaanim_core::kurbo::PathEl::MoveTo(point))
                if (point.x + 10.0).abs() < 1e-9 && (point.y + 20.0).abs() < 1e-9
        ));
        assert!(matches!(
            elements.last(),
            Some(gaanim_core::kurbo::PathEl::LineTo(point))
                if (point.x - 80.0).abs() < 1e-9 && (point.y - 60.0).abs() < 1e-9
        ));
    }

    #[test]
    fn deferred_group_fade_in_reveals_deferred_children() {
        let mut canvas = SceneModel::new(320, 180);
        let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
        let child = canvas.dot(8.0);
        child.follow_to(&anchor, 0.0, 24.0);
        let group = canvas.group(&[&child]);
        canvas.play(vec![group.fade_in(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let opacity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            let mut query = world.query::<(&gaanim_scene::MobjectId, &Opacity)>();
            query
                .iter(world)
                .find(|(object_id, _)| object_id.0 == compiled_id)
                .map(|(_, opacity)| opacity.0)
                .expect("group member should compile into a mobject")
        };

        assert_eq!(opacity_for(&mut world, group.id), 0.0);
        assert_eq!(opacity_for(&mut world, child.id), 0.0);

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);

        assert!(opacity_for(&mut world, group.id) > 0.0);
        assert!(opacity_for(&mut world, child.id) > 0.0);
    }

    #[test]
    fn moving_a_group_does_not_reveal_unentered_deferred_children() {
        let mut canvas = SceneModel::new(320, 180);
        let anchor = canvas.dot(8.0).move_to(-60.0, 0.0);
        let mass = canvas.dot(8.0).move_to(60.0, 0.0);
        let spring = canvas.spring_between(
            CanvasEndpoint::Entity(anchor.id),
            CanvasEndpoint::Entity(mass.id),
            6,
            10.0,
        );
        let group = canvas.group(&[&anchor, &mass, &spring]);
        canvas.play(vec![group.animate().shift_by(40.0, 0.0).duration(1.0)]);
        canvas.play(vec![spring.fade_in(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let group_id = gaanim_core::ObjectId::from_raw(group.id.as_raw() - 1);
        let group_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == group_id).then_some(entity))
            .expect("compiled group");
        assert_eq!(
            world.get::<Opacity>(group_entity).unwrap().0,
            1.0,
            "a deferred child must not hide the group or its already-visible siblings"
        );

        let spring_id = gaanim_core::ObjectId::from_raw(spring.id.as_raw() - 1);
        let spring_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == spring_id).then_some(entity))
            .expect("compiled deferred spring");
        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world.get::<Opacity>(spring_entity).unwrap().0,
            0.0,
            "the parent movement must not serve as the spring entry animation"
        );

        timeline.seek(&mut world, 1.5);
        let entered = world.get::<Opacity>(spring_entity).unwrap().0;
        assert!(entered > 0.0 && entered < 1.0);
    }

    #[test]
    fn moving_a_group_to_a_target_carries_a_fixed_support_and_ordinary_member_together() {
        let mut canvas = SceneModel::new(320, 180);
        let support = canvas.support_at(
            CanvasEndpoint::Static(DVec3::new(-60.0, 40.0, 0.0)),
            "fixed",
            DVec3::Y,
            48.0,
            70.0,
            None,
        );
        let mass = canvas.dot(8.0).move_to(60.0, -40.0);
        let group = canvas.group(&[&support.drawable, &mass]);
        canvas.play(vec![group.animate().move_to(40.0, 0.0).duration(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let entity_for = |world: &mut World, id: gaanim_core::ObjectId| {
            let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
            world
                .query::<(bevy::prelude::Entity, &MobjectId)>()
                .iter(world)
                .find_map(|(entity, object_id)| (object_id.0 == compiled_id).then_some(entity))
                .expect("compiled group member")
        };
        let support_entity = entity_for(&mut world, support.drawable.id);
        let mass_entity = entity_for(&mut world, mass.id);
        let before_support = gaanim_animation::resolve_tracking_endpoint(
            &gaanim_animation::TrackingEndpoint::Entity(support_entity),
            &world,
        )
        .unwrap();
        let before_mass = gaanim_animation::resolve_tracking_endpoint(
            &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
            &world,
        )
        .unwrap();

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.5);
        gaanim_animation::endpoint_follow_system(&mut world);

        let after_support = gaanim_animation::resolve_tracking_endpoint(
            &gaanim_animation::TrackingEndpoint::Entity(support_entity),
            &world,
        )
        .unwrap();
        let after_mass = gaanim_animation::resolve_tracking_endpoint(
            &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
            &world,
        )
        .unwrap();
        assert!((after_support.x - before_support.x) > 1.0);
        assert_eq!(
            after_support - before_support,
            after_mass - before_mass,
            "fixed support and mass must share the parent movement"
        );
    }

    #[test]
    fn writing_or_creating_a_group_keeps_updater_coordinates_and_reveals_deferred_members() {
        for use_write in [true, false] {
            let mut canvas = SceneModel::new(320, 180);
            let mass = canvas.dot(8.0).move_to(60.0, -40.0);
            mass.add_custom_updater(gaanim_animation::Updater::new(
                |_dt, _elapsed, entity, world| {
                    if let Some(mut transform) = world.get_mut::<SpatialTransform>(entity) {
                        transform.translation = DVec3::new(60.0, -40.0, 0.0);
                    }
                    true
                },
            ));
            let trace = canvas.traced_path_with_options(&mass, Some(1.0), Some(100), 0.5);
            let group = canvas.group(&[&mass, &trace]);
            let entry = if use_write {
                group.write(1.0)
            } else {
                group.create(1.0)
            };
            canvas.play(vec![entry]);

            let mut world = World::new();
            world.insert_resource(Timeline::new());
            world.insert_resource(gaanim_animation::PlaybackState::default());
            world.insert_resource(gaanim_text::font::FontRegistry::new());
            world.insert_resource(gaanim_text::prelude::TextConfig::default());
            canvas.compile(&mut world);
            world.flush();

            let entity_for = |world: &mut World, id: gaanim_core::ObjectId| {
                let compiled_id = gaanim_core::ObjectId::from_raw(id.as_raw() - 1);
                world
                    .query::<(bevy::prelude::Entity, &MobjectId)>()
                    .iter(world)
                    .find_map(|(entity, object_id)| (object_id.0 == compiled_id).then_some(entity))
                    .expect("compiled group member")
            };
            let mass_entity = entity_for(&mut world, mass.id);
            let trace_entity = entity_for(&mut world, trace.id);
            let snapshot = WorldSnapshot::capture(&mut world);
            let mut timeline = world.remove_resource::<Timeline>().unwrap();
            timeline.add_keyframe(0.0, snapshot);
            timeline.seek(&mut world, 0.5);
            gaanim_animation::seek_updaters(&mut world, 0.5);

            let position = gaanim_animation::resolve_tracking_endpoint(
                &gaanim_animation::TrackingEndpoint::Entity(mass_entity),
                &world,
            )
            .unwrap();
            assert_eq!(position, DVec3::new(60.0, -40.0, 0.0));
            assert!(
                world.get::<Opacity>(trace_entity).unwrap().0 > 0.0,
                "Write/Create on the group is an explicit entry for deferred descendants"
            );
        }
    }

    #[test]
    fn paper_theme_uses_dark_text_fills() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = SceneModel::new(1280, 720);
        canvas
            .set_theme("paper")
            .expect("paper is a built-in theme");
        let config = canvas.themed_text_config();

        for role in [
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Heading,
            TextRole::Body,
            TextRole::Caption,
            TextRole::Label,
            TextRole::Math,
            TextRole::Code,
        ] {
            assert_eq!(config.roles[&role].fill_color, Color::BLACK);
        }
    }

    #[test]
    fn direct_font_overrides_win_over_themes() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = SceneModel::new(1280, 720);
        canvas
            .set_fonts(
                Some("Inter".into()),
                Some("STIX Two Math".into()),
                Some("JetBrains Mono".into()),
            )
            .unwrap();
        canvas.set_theme("paper").unwrap();

        let config = canvas.themed_text_config();
        assert_eq!(config.roles[&TextRole::Body].font_family, "Inter");
        assert_eq!(config.roles[&TextRole::Math].font_family, "STIX Two Math");
        assert_eq!(config.roles[&TextRole::Code].font_family, "JetBrains Mono");
    }

    #[test]
    fn reactive_annotations_share_one_default_text_size() {
        let mut canvas = SceneModel::new(1920, 1080);
        let origin = CanvasEndpoint::Static(DVec3::ZERO);
        let tip = CanvasEndpoint::Static(DVec3::new(120.0, 0.0, 0.0));
        let angle = canvas
            .angle_between_with_options(
                origin.clone(),
                CanvasRay::Direction(DVec3::X),
                CanvasRay::Direction(DVec3::Y),
                64.0,
                AngleDimensionOptions {
                    label: Some("$theta$".to_owned()),
                    show_value: true,
                    ..Default::default()
                },
            )
            .unwrap();
        let vector = canvas
            .vector_between_with_parts(
                origin,
                tip,
                Some("$F$".to_owned()),
                true,
                ".1f".to_owned(),
                Some("N".to_owned()),
                1.0,
                14.0,
                None,
                None,
            )
            .unwrap();

        for part in [
            angle.label.as_ref(),
            angle.unit.as_ref(),
            vector.label.as_ref(),
            vector.unit.as_ref(),
        ] {
            assert_eq!(
                part.expect("reactive annotation text")
                    .text_spec()
                    .expect("reactive annotation text spec")
                    .style
                    .size,
                Some(DEFAULT_REACTIVE_TEXT_SIZE)
            );
        }

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let sizes = world
            .query::<&gaanim_animation::ReactiveReadout>()
            .iter(&world)
            .map(|readout| readout.font_size)
            .collect::<Vec<_>>();
        assert_eq!(sizes.len(), 2);
        assert!(sizes.iter().all(|size| *size == DEFAULT_REACTIVE_TEXT_SIZE));
    }

    #[test]
    fn mechanism_create_and_write_start_with_every_reactive_path_hidden() {
        let mut canvas = SceneModel::new(640, 360);
        let pivot = CanvasEndpoint::Static(DVec3::new(-120.0, 80.0, 0.0));
        let support = canvas.support_at(pivot.clone(), "fixed", DVec3::NEG_Y, 48.0, 72.0, None);
        let angle = canvas
            .angle_between_with_options(
                pivot.clone(),
                CanvasRay::Direction(DVec3::NEG_Y),
                CanvasRay::Direction(DVec3::new(0.7, -0.7, 0.0)),
                64.0,
                AngleDimensionOptions {
                    show_value: true,
                    ..Default::default()
                },
            )
            .expect("valid angle");
        let force = canvas
            .force_at(
                pivot,
                ScalarSource::constant(981.0),
                ScalarSource::constant(-std::f64::consts::FRAC_PI_2),
                0.1,
                Some("$P$".to_owned()),
                true,
                ".0f".to_owned(),
                Some("N".to_owned()),
                40.0,
                None,
                None,
            )
            .expect("valid force");
        canvas.play(vec![
            support.drawable.write(1.0),
            angle.drawable.create(1.0),
            force.drawable.write(1.0),
        ]);

        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();

        let draw_paths = app
            .world_mut()
            .query::<(&gaanim_animation::PathReveal, &gaanim_scene::Path2D)>()
            .iter(app.world())
            .filter(|(reveal, _)| reveal.0 == 0.0)
            .map(|(_, path)| path.0.is_empty())
            .collect::<Vec<_>>();
        assert!(draw_paths.len() > 3, "expected composite draw leaves");
        assert!(
            draw_paths.iter().all(|empty| *empty),
            "fixed support, angle, and force paths must not appear before their entry animation"
        );

        let readout_paths = app
            .world_mut()
            .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::Path2D)>()
            .iter(app.world())
            .filter(|(tag, _)| tag.0 == "SvgPath#ReactiveReadout")
            .map(|(_, path)| path.0.is_empty())
            .collect::<Vec<_>>();
        assert_eq!(readout_paths.len(), 2);
        assert!(
            readout_paths.iter().all(|empty| *empty),
            "angle and force values must remain hidden until Create/Write reveals them"
        );
    }

    #[test]
    fn angle_color_applies_to_the_reactive_numeric_value() {
        let mut canvas = SceneModel::new(640, 360);
        let gold = Color::from_rgb8(255, 200, 0);
        let angle = canvas
            .angle_between_with_options(
                CanvasEndpoint::Static(DVec3::ZERO),
                CanvasRay::Direction(DVec3::X),
                CanvasRay::Direction(DVec3::Y),
                64.0,
                AngleDimensionOptions {
                    label: Some("$theta$".to_owned()),
                    show_value: true,
                    color: Some(gold),
                    ..Default::default()
                },
            )
            .unwrap();
        for text_part in [angle.label.as_ref(), angle.unit.as_ref()] {
            assert_eq!(
                text_part
                    .expect("angle annotation text")
                    .text_spec()
                    .expect("angle annotation text spec")
                    .style
                    .color,
                Some(gold),
                "the explicit angle color must override the theme for symbols and units"
            );
        }
        let _number = angle.number.as_ref().expect("numeric angle readout");

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut style_schedule = bevy::prelude::Schedule::default();
        style_schedule.add_systems(gaanim_scene::systems::style_propagation_system);
        style_schedule.run(&mut world);

        let fill = world
            .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::FillBrush)>()
            .iter(&world)
            .find_map(|(tag, fill)| (tag.0 == "SvgPath#ReactiveReadout").then_some(fill))
            .expect("compiled angle number fill");
        assert!(matches!(
            fill.0.as_ref(),
            Some(Brush::Solid(color)) if *color == gold
        ));

        let snapshot = WorldSnapshot::capture(&mut world);
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, snapshot);
        timeline.seek(&mut world, 0.0);
        let fill_after_seek = world
            .query::<(&gaanim_scene::ObjectTag, &gaanim_scene::FillBrush)>()
            .iter(&world)
            .find_map(|(tag, fill)| (tag.0 == "SvgPath#ReactiveReadout").then_some(fill))
            .expect("angle number fill after seek");
        assert!(matches!(
            fill_after_seek.0.as_ref(),
            Some(Brush::Solid(color)) if *color == gold
        ));
    }

    #[test]
    fn force_color_applies_to_the_reactive_numeric_value_after_fade_and_seek() {
        let mut canvas = SceneModel::new(640, 360);
        canvas
            .set_theme("technical")
            .expect("technical is a built-in theme");
        let green = Color::from_rgb8(75, 229, 124);
        let force = canvas
            .force_at(
                CanvasEndpoint::Static(DVec3::ZERO),
                ScalarSource::constant(45.0),
                ScalarSource::constant(0.4),
                2.0,
                Some("$F$".to_owned()),
                true,
                ".1f".to_owned(),
                Some("N".to_owned()),
                28.0,
                None,
                Some(green),
            )
            .expect("valid force");
        canvas.play(vec![force.drawable.fade_in(1.0)]);

        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.0);
        app.update();

        let (fill, stroke) = app
            .world_mut()
            .query::<(
                &gaanim_scene::ObjectTag,
                &gaanim_scene::FillBrush,
                &gaanim_scene::StrokeBrush,
            )>()
            .iter(app.world())
            .find_map(|(tag, fill, stroke)| {
                (tag.0 == "SvgPath#ReactiveReadout").then_some((fill, stroke))
            })
            .expect("compiled force number style after seek");
        assert!(matches!(
            fill.0.as_ref(),
            Some(Brush::Solid(color)) if *color == green
        ));
        assert!(
            stroke.brush.is_none(),
            "a reactive number is filled text and must not inherit the plot stroke"
        );
    }

    #[test]
    fn presentation_theme_uses_projector_contrast() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = SceneModel::new(1920, 1080);
        canvas
            .set_theme("presentation")
            .expect("presentation is a built-in theme");
        let config = canvas.themed_text_config();

        assert_eq!(canvas.background, Some(Color::from_rgb8(0x07, 0x0B, 0x16)));
        assert_eq!(
            config.roles[&TextRole::Title].fill_color,
            Color::from_rgb8(0xFF, 0xD1, 0x66)
        );
        assert_eq!(
            config.roles[&TextRole::Body].fill_color,
            Color::from_rgb8(0xF4, 0xF7, 0xFB)
        );
    }

    #[test]
    fn known_color_scheme_drives_text_and_components_from_one_palette() {
        use gaanim_text::prelude::TextRole;

        let mut canvas = SceneModel::new(1920, 1080);
        canvas.set_theme("dracula").expect("dracula is built in");
        let theme = canvas.theme_style.as_ref().unwrap();

        assert_eq!(canvas.theme.as_deref(), Some("dracula"));
        assert_eq!(canvas.background, Some(Color::from_rgb8(0x28, 0x2A, 0x36)));
        assert_eq!(
            theme.text.roles[&TextRole::Title].fill_color,
            theme.palette.title
        );
        assert_eq!(
            theme.text.roles[&TextRole::Body].fill_color,
            theme.palette.foreground
        );
    }

    #[test]
    fn explicit_background_survives_later_theme_changes() {
        let explicit = Color::from_rgb8(0x12, 0x34, 0x56);
        let mut canvas = SceneModel::new(640, 360);
        canvas.set_background(Some(explicit));
        canvas.set_theme("paper").unwrap();
        assert_eq!(canvas.background, Some(explicit));
    }

    #[test]
    fn gradient_background_keeps_a_representative_color_for_theme_contrast() {
        let navy = Color::from_rgb8(0x0B, 0x10, 0x20);
        let blue = Color::from_rgb8(0x25, 0x63, 0xEB);
        let gradient = gaanim_core::peniko::Gradient::new_linear((-320.0, 0.0), (320.0, 0.0))
            .with_stops([(0.0, navy), (1.0, blue)]);
        let mut canvas = SceneModel::new(640, 360)
            .background_brush(gaanim_core::peniko::Brush::Gradient(gradient));

        assert_eq!(canvas.background, Some(navy));
        assert!(matches!(
            canvas.background_paint,
            Some(gaanim_renderer::background::BackgroundPaint::Brush(
                gaanim_core::peniko::Brush::Gradient(_)
            ))
        ));

        canvas.set_theme("paper").unwrap();
        assert_eq!(canvas.background, Some(navy));
    }

    #[test]
    fn theme_classes_propagate_through_nested_groups() {
        let mut canvas = SceneModel::new(640, 360);
        let first = canvas.circle(20.0);
        let second = canvas.square(30.0);
        let inner = canvas.group(&[&first, &second]);
        let outer = canvas.group(&[&inner]);
        outer.style_class("focus").unwrap();
        assert_eq!(
            first
                .spec
                .lock()
                .expect("first spec poisoned")
                .style_classes,
            ["focus"]
        );
        assert_eq!(
            second
                .spec
                .lock()
                .expect("second spec poisoned")
                .style_classes,
            ["focus"]
        );
    }

    #[test]
    fn derived_theme_can_override_semantic_colors_and_fonts() {
        use std::collections::HashMap;

        use gaanim_text::prelude::TextRole;

        let mut theme = CanvasTheme::builtin("nord").unwrap();
        theme.name = "research-lab".into();
        theme
            .set_colors(&HashMap::from([(
                "accent".into(),
                Color::from_rgb8(0xFF, 0x00, 0x66),
            )]))
            .unwrap();
        theme
            .set_fonts(&HashMap::from([("text".into(), "Inter".into())]))
            .unwrap();

        assert_eq!(theme.palette.accent, Color::from_rgb8(0xFF, 0x00, 0x66));
        for role in [
            TextRole::Title,
            TextRole::Subtitle,
            TextRole::Heading,
            TextRole::Body,
            TextRole::Caption,
            TextRole::Label,
        ] {
            assert_eq!(theme.text.roles[&role].font_family, "Inter");
        }
        assert_ne!(theme.text.roles[&TextRole::Math].font_family, "Inter");
    }

    #[test]
    fn theme_tokens_are_queryable_and_low_contrast_is_reported() {
        use std::collections::HashMap;

        let mut theme = CanvasTheme::builtin("presentation").unwrap();
        assert_eq!(theme.color("border").unwrap(), theme.palette.rule);
        assert_eq!(theme.color("primary").unwrap(), theme.palette.foreground);
        assert!(theme.validate().is_empty());

        theme
            .set_colors(&HashMap::from([(
                "foreground".into(),
                theme.palette.background,
            )]))
            .unwrap();
        assert!(
            theme
                .validate()
                .iter()
                .any(|warning| warning.contains("foreground on background"))
        );
    }

    #[test]
    fn svg_parts_are_addressable_and_group_styles_reach_descendant_paths() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_svg_parts_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = SceneModel::new(320, 180);
        let svg = canvas.svg(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();

        assert_eq!(
            canvas
                .state
                .lock()
                .expect("canvas state poisoned")
                .all_drawables
                .len(),
            1,
            "only the SVG root should be a top-level drawable"
        );

        let red = Color::from_rgb8(255, 0, 0);
        svg.part("assembly").unwrap().fill(red);
        for id in ["body", "joint"] {
            let part = svg.part(id).unwrap();
            let spec = part.spec.lock().expect("SVG path spec poisoned");
            assert!(spec.fill_overridden);
            assert!(matches!(
                spec.fill,
                Some(gaanim_core::peniko::Brush::Solid(color)) if color == red
            ));
        }

        assert!(matches!(
            svg.part("missing"),
            Err(crate::canvas::SvgPartError::Unknown { available, .. })
                if available == "assembly, body, joint"
        ));
        assert!(matches!(
            canvas.circle(10.0).part("body"),
            Err(crate::canvas::SvgPartError::NotSvg)
        ));
    }

    #[test]
    fn scaled_svg_group_stroke_remains_in_logical_scene_units() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_svg_logical_stroke_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <rect id="body" x="10" y="10" width="80" height="80" fill="#ffffff"/>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = SceneModel::new(16.0, 9.0);
        let svg = canvas
            .svg(&temp)
            .unwrap()
            .scale_to(0.01)
            .stroke(Color::BLACK, 0.025);
        canvas.play(vec![
            svg.animate()
                .stroke(Color::from_rgb8(0x80, 0x80, 0x80), 0.0167)
                .duration(1.0),
        ]);
        std::fs::remove_file(temp).unwrap();

        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.0);
        app.update();

        let effective_width = app
            .world_mut()
            .query::<(
                &gaanim_scene::ObjectTag,
                &gaanim_scene::StrokeBrush,
                &gaanim_math::GlobalSpatialTransform,
            )>()
            .iter(app.world())
            .find_map(|(tag, stroke, transform)| {
                (tag.0 == "SvgPath#body").then(|| {
                    let [a, b, c, d, _, _] = transform.affine_2d.as_coeffs();
                    let scale = ((a.hypot(b)) * (c.hypot(d))).sqrt();
                    stroke.style.width * scale
                })
            })
            .expect("compiled SVG body");
        assert!((effective_width - 0.0167).abs() < 1.0e-9);
    }

    #[test]
    fn scaled_svg_clip_mask_inherits_the_visible_hierarchy_transform() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_svg_scaled_clip_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <defs><clipPath id="window"><rect x="10" y="10" width="80" height="80"/></clipPath></defs>
                <g clip-path="url(#window)"><rect width="100" height="100" fill="#00aaff"/></g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = SceneModel::new(320, 180);
        canvas.svg(&temp).unwrap().scale_to(2.0);
        std::fs::remove_file(temp).unwrap();

        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();

        let (target_scale, source) = app
            .world_mut()
            .query::<(
                &gaanim_renderer::effects::ClipMask,
                &gaanim_math::GlobalSpatialTransform,
            )>()
            .iter(app.world())
            .find_map(|(mask, target)| mask.sources.first().copied().map(|source| (target, source)))
            .expect("compiled SVG clip mask and source");
        let source_scale = app
            .world()
            .get::<gaanim_math::GlobalSpatialTransform>(source)
            .expect("clip source transform");

        let target_coeffs = target_scale.affine_2d.as_coeffs();
        let source_coeffs = source_scale.affine_2d.as_coeffs();
        assert!((target_coeffs[0].abs() - source_coeffs[0].abs()).abs() < 1e-9);
        assert!((target_coeffs[3].abs() - source_coeffs[3].abs()).abs() < 1e-9);
    }

    #[test]
    fn svg_clip_mask_is_not_consumed_by_group_create_animation() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_svg_create_clip_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="100" height="100" xmlns="http://www.w3.org/2000/svg">
                <defs><clipPath id="window"><rect x="10" y="10" width="80" height="80"/></clipPath></defs>
                <g clip-path="url(#window)"><rect width="100" height="100" fill="#00aaff"/></g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = SceneModel::new(320, 180);
        let svg = canvas.svg(&temp).unwrap();
        canvas.play(vec![svg.create(1.0)]);
        std::fs::remove_file(temp).unwrap();

        let mut world = bevy::prelude::World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_animation::PlaybackState::default());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let source = world
            .query::<&gaanim_renderer::effects::ClipMask>()
            .iter(&world)
            .find_map(|mask| mask.sources.first().copied())
            .expect("compiled SVG clip source");
        let source_path = world
            .get::<gaanim_scene::components::Path2D>(source)
            .expect("clip source path");
        assert!(!source_path.0.elements().is_empty());
        assert!(world.get::<gaanim_animation::PathReveal>(source).is_none());
        assert!(
            world
                .get::<gaanim_animation::FillDrawProgress>(source)
                .is_none()
        );
    }

    #[test]
    fn stagger_offsets_delays_and_cursor() {
        let mut canvas = SceneModel::new(1280, 720);
        let first = canvas.circle(20.0);
        let second = canvas.circle(20.0);

        let plan = Composition::stagger(
            vec![
                Composition::leaf(first.fade_in(1.0)),
                Composition::leaf(second.fade_in(1.0)),
            ],
            0.25,
        )
        .unwrap();
        canvas
            .play_composition_configured(plan, None, None)
            .unwrap();

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 1.25).abs() < 1e-9);

        let Some(Op::Play(anims)) = segment.ops.last() else {
            panic!("expected parallel play op");
        };
        assert_eq!(anims.len(), 2);
        assert!((anims[0].delay - 0.0).abs() < 1e-9);
        assert!((anims[1].delay - 0.25).abs() < 1e-9);
    }

    #[test]
    fn compound_property_animation_stays_pure_and_schedules_as_one_anim() {
        let mut canvas = SceneModel::new(1280, 720);
        let shape = canvas.circle(20.0).fill(Color::WHITE);

        let pending = shape.animate().duration(2.0);
        {
            let guard = canvas.state.lock().expect("canvas state poisoned");
            assert_eq!(guard.active().cursor, 0.0);
        }

        let animation = pending
            .move_to_3d(12.0, 24.0, 3.0)
            .scale_to_3d(2.0, 3.0, 4.0)
            .fill(Color::from_rgb8(20, 40, 220))
            .stroke(Color::WHITE, 5.0)
            .opacity(0.6);
        {
            let guard = canvas.state.lock().expect("canvas state poisoned");
            assert_eq!(guard.active().cursor, 0.0);
            let AnimationType::Properties(properties) = &animation.inner.anim_type else {
                panic!("expected typed property animation");
            };
            assert!(properties.translation.is_some());
            assert!(properties.scale.is_some());
            assert!(properties.fill.is_some());
            assert!(properties.stroke_color.is_some());
            assert_eq!(properties.stroke_width, Some(5.0));
            assert_eq!(properties.opacity, Some(0.6));
        }

        canvas.play(vec![animation]);
        let guard = canvas.state.lock().expect("canvas state poisoned");
        assert_eq!(guard.active().cursor, 2.0);
        let Some(Op::Play(anims)) = guard.active().ops.last() else {
            panic!("scene.play should regroup the compound animation");
        };
        assert_eq!(anims.len(), 1);
        assert!(matches!(anims[0].anim_type, AnimationType::Properties(_)));
    }

    #[test]
    fn move_to_anchor_places_the_requested_anchor_at_the_target() {
        let mut canvas = SceneModel::new(640, 360);
        let rect = canvas.rect(100.0, 60.0).move_to(-120.0, 0.0);
        canvas.play(vec![
            rect.move_to_anchor(80.0, 40.0, Anchor::TopRight)
                .duration(1.0),
        ]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| {
                (id.0 == gaanim_core::ObjectId::from_raw(rect.id.as_raw() - 1)).then_some(entity)
            })
            .expect("rectangle entity");
        let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 1.0);

        let translation = world
            .get::<SpatialTransform>(entity)
            .expect("rectangle transform")
            .translation;
        assert_eq!(translation, gaanim_core::glam::DVec3::new(30.0, 10.0, 0.0));
    }

    #[test]
    fn move_to_anchor_point_animates_center_to_reference_anchor() {
        let mut canvas = SceneModel::new(640, 360);
        let reference = canvas.rect(100.0, 40.0).move_to(50.0, 20.0);
        let moving = canvas.rect(10.0, 6.0).move_to(-100.0, -80.0);
        let point = reference.anchor_point(
            Anchor::TopRight,
            gaanim_core::glam::DVec3::new(-5.0, -3.0, 0.0),
        );
        canvas.play(vec![
            moving
                .animate()
                .move_to_anchor_point(point)
                .unwrap()
                .duration(1.0),
        ]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let mut timeline = world.remove_resource::<Timeline>().expect("timeline");
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 1.0);

        let mut roots = world
            .query::<(
                &gaanim_scene::LocalBounds,
                &SpatialTransform,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .filter_map(|(bounds, transform, parent)| {
                parent.is_none().then_some((bounds.0, *transform))
            })
            .collect::<Vec<_>>();
        roots.sort_by(|(left, _), (right, _)| left.width().total_cmp(&right.width()));
        let (moving_bounds, moving_transform) = roots[0];
        let (reference_bounds, reference_transform) = roots[1];
        let expected = reference_transform.to_mat4().transform_point3(
            Anchor::TopRight.get_point(&reference_bounds)
                + gaanim_core::glam::DVec3::new(-5.0, -3.0, 0.0),
        );
        let actual = moving_transform
            .to_mat4()
            .transform_point3(moving_bounds.center());
        assert!(
            actual.distance(expected) < 1e-6,
            "expected {expected:?}, got {actual:?}"
        );
    }

    #[test]
    fn primitive_3d_color_property_preserves_other_material_channels() {
        let mut canvas = SceneModel::new(640, 360);
        let original = gaanim_scene::Material3D::metal(Color::WHITE);
        let cube = canvas.cube(2.0, original).expect("valid cube");
        let target_color = Color::from_rgb8(32, 96, 224);

        let animation = cube
            .animate()
            .color(target_color)
            .rotate_to_3d(0.2, 0.4, 0.6);
        let AnimationType::Properties(properties) = &animation.inner.anim_type else {
            panic!("expected typed property animation");
        };
        let Some((from, to)) = properties.material else {
            panic!("Primitive3D color should target Material3D");
        };
        assert_eq!(from, original);
        assert_eq!(to.color, target_color);
        assert_eq!(to.roughness, original.roughness);
        assert_eq!(to.metallic, original.metallic);
        assert_eq!(to.emissive, original.emissive);
        assert_eq!(to.emissive_strength, original.emissive_strength);
        assert!(properties.rotation.is_some());
        assert!(properties.fill.is_none());
    }

    #[test]
    fn camera_animation_can_be_regrouped_with_drawables() {
        let mut canvas = SceneModel::new(1280, 720);
        let marker = canvas.circle(20.0);
        let marker_anim = marker.fade_in(2.0);
        let camera_anim = canvas
            .camera_orbit(0.5, 0.1, 2.0)
            .rate_func(gaanim_math::RateFunc::Linear)
            .delay(0.25);

        canvas.play(vec![marker_anim, camera_anim]);

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 2.25).abs() < 1e-9);
        let Some(Op::Play(anims)) = segment.ops.last() else {
            panic!("expected parallel play op");
        };
        assert_eq!(anims.len(), 2);
        let camera = anims
            .iter()
            .find(|anim| matches!(anim.anim_type, AnimationType::CameraOrbit { .. }))
            .expect("camera orbit in parallel batch");
        assert!(matches!(camera.rate_func, gaanim_math::RateFunc::Linear));
        assert!((camera.delay - 0.25).abs() < 1e-9);
    }

    #[test]
    fn play_builders_counts_existing_delays_in_cursor() {
        let mut canvas = SceneModel::new(1280, 720);
        let first = canvas.circle(20.0);
        let second = canvas.circle(20.0);

        canvas.play(vec![
            first.fade_in(1.0).delay(0.1),
            second.fade_in(1.0).delay(0.4),
        ]);

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert!((segment.cursor - 1.4).abs() < 1e-9);
    }

    #[test]
    fn scene_object_commands_deduplicate_and_reject_foreign_drawables() {
        let mut canvas = SceneModel::new(1280, 720);
        let title = canvas.test_title("Persistent title");
        canvas.segment("next", None).unwrap();
        canvas
            .reuse_many(&[title.clone(), title.clone()])
            .expect("same-scene drawable should be reusable");

        let guard = canvas.state.lock().expect("canvas state poisoned");
        let segment = guard.active();
        assert_eq!(
            segment
                .mobject_ids
                .iter()
                .filter(|id| **id == title.id)
                .count(),
            1
        );
        assert_eq!(
            segment
                .ops
                .iter()
                .filter(|op| matches!(op, Op::Reuse(id) if *id == title.id))
                .count(),
            1
        );
        drop(guard);

        let mut other = SceneModel::new(1280, 720);
        let foreign = other.circle(20.0);
        assert!(matches!(
            canvas.persist(&foreign),
            Err(SceneObjectError::ForeignScene)
        ));
    }

    #[test]
    fn reuse_persist_and_release_schedule_reversible_scene_membership() {
        let mut canvas = SceneModel::new(1280, 720);
        let title = canvas.test_title("Shared title");
        canvas.wait(1.0);

        canvas
            .segment("reused", Some(TransitionType::CrossFade { duration: 0.5 }))
            .unwrap();
        canvas.reuse(&title).unwrap();
        canvas.wait(0.6);
        canvas.persist(&title).unwrap();
        canvas.wait(0.4);

        canvas
            .segment(
                "released",
                Some(TransitionType::Slide {
                    duration: 0.5,
                    direction: gaanim_timeline::transition::SlideDirection::Left,
                }),
            )
            .unwrap();
        canvas.release(&title).unwrap();
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let (initial_scene, reused_scene, released_scene) = {
            let timeline = world.resource::<Timeline>();
            let scene = |name: &str| {
                timeline
                    .scenes
                    .values()
                    .find(|scene| scene.name == name)
                    .map(|scene| scene.id)
                    .unwrap()
            };
            (scene("_default"), scene("reused"), scene("released"))
        };
        let title_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == title.id).then_some(entity))
            .expect("title entity");

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(initial_scene)
        );

        timeline.seek(&mut world, 1.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
        assert_eq!(
            world.get::<gaanim_scene::Opacity>(title_entity).unwrap().0,
            1.0
        );
        let hierarchy_memberships: Vec<_> = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
            .collect();
        assert!(
            hierarchy_memberships.len() > 1,
            "title should contain glyphs"
        );
        assert!(hierarchy_memberships.iter().all(Option::is_none));

        timeline.seek(&mut world, 1.55);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(reused_scene)
        );

        timeline.seek(&mut world, 1.75);
        assert_eq!(world.get::<SceneMember>(title_entity), None);

        timeline.seek(&mut world, 2.0);
        let released_start_x = world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x;
        timeline.seek(&mut world, 2.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert_eq!(
            world
                .get::<gaanim_math::SpatialTransform>(title_entity)
                .unwrap()
                .translation
                .x,
            released_start_x
        );

        timeline.seek(&mut world, 2.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(released_scene)
        );

        timeline.seek(&mut world, 0.5);
        assert_eq!(
            world
                .get::<SceneMember>(title_entity)
                .map(|member| member.0),
            Some(initial_scene)
        );

        assert_ne!(reused_scene, released_scene);
    }

    #[test]
    fn reuse_from_a_nonadjacent_segment_enters_with_the_destination() {
        let mut canvas = SceneModel::new(640, 360);
        let marker = canvas.circle(24.0);
        canvas.wait(0.5);
        canvas.segment("middle", Some(TransitionType::Cut)).unwrap();
        canvas.wait(0.5);
        canvas
            .segment("return", Some(TransitionType::CrossFade { duration: 0.4 }))
            .unwrap();
        canvas.reuse(&marker).unwrap();
        canvas.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let return_scene = world
            .resource::<Timeline>()
            .scenes
            .values()
            .find(|scene| scene.name == "return")
            .map(|scene| scene.id)
            .unwrap();
        let marker_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.2);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(return_scene)
        );
        let opacity = world.get::<gaanim_scene::Opacity>(marker_entity).unwrap().0;
        assert!((opacity - 0.5).abs() < 1e-5);
    }

    #[test]
    fn persistent_object_stays_fixed_for_the_entire_slide_transition() {
        let mut canvas = SceneModel::new(640, 360);
        let title = canvas.test_title("KEEP");
        canvas.wait(1.0);
        canvas.persist(&title).unwrap();
        canvas
            .segment(
                "next",
                Some(TransitionType::Slide {
                    duration: 1.0,
                    direction: gaanim_timeline::transition::SlideDirection::Left,
                }),
            )
            .unwrap();
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let title_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.25);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        let first_x = world
            .get::<gaanim_math::SpatialTransform>(title_entity)
            .unwrap()
            .translation
            .x;
        timeline.seek(&mut world, 1.75);
        assert_eq!(world.get::<SceneMember>(title_entity), None);
        assert_eq!(
            world
                .get::<gaanim_math::SpatialTransform>(title_entity)
                .unwrap()
                .translation
                .x,
            first_x
        );
        assert!(world.get::<gaanim_scene::Visible>(title_entity).is_some());
    }

    #[test]
    fn late_reuse_changes_membership_at_the_current_cursor() {
        let mut canvas = SceneModel::new(640, 360);
        let marker = canvas.circle(24.0);
        canvas.wait(1.0);
        canvas
            .segment("second", Some(TransitionType::CrossFade { duration: 0.5 }))
            .unwrap();
        canvas.wait(0.25);
        canvas.reuse(&marker).unwrap();
        canvas.wait(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let (first_scene, second_scene) = {
            let timeline = world.resource::<Timeline>();
            let scene = |name: &str| {
                timeline
                    .scenes
                    .values()
                    .find(|scene| scene.name == name)
                    .map(|scene| scene.id)
                    .unwrap()
            };
            (scene("_default"), scene("second"))
        };
        let marker_entity = world
            .query::<(
                bevy::prelude::Entity,
                &MobjectId,
                Option<&bevy::prelude::ChildOf>,
            )>()
            .iter(&world)
            .find_map(|(entity, _, parent)| parent.is_none().then_some(entity))
            .unwrap();
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );

        timeline.seek(&mut world, 1.2);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(first_scene)
        );
        timeline.seek(&mut world, 1.3);
        assert_eq!(
            world
                .get::<SceneMember>(marker_entity)
                .map(|member| member.0),
            Some(second_scene)
        );
    }

    #[test]
    fn persistence_covers_group_and_svg_descendants() {
        let temp = std::env::temp_dir().join(format!(
            "gaanim_persistent_svg_api_test_{}.svg",
            std::process::id()
        ));
        std::fs::write(
            &temp,
            r##"<svg width="80" height="40" xmlns="http://www.w3.org/2000/svg">
                <g id="assembly">
                  <rect id="body" width="30" height="20" fill="#0000ff"/>
                  <circle id="joint" cx="50" cy="20" r="8" fill="#00ff00"/>
                </g>
              </svg>"##,
        )
        .unwrap();

        let mut canvas = SceneModel::new(640, 360);
        let left = canvas.circle(20.0);
        let right = canvas.square(40.0);
        let group = canvas.group(&[&left, &right]);
        let svg = canvas.svg(&temp).unwrap();
        std::fs::remove_file(temp).unwrap();
        canvas.persist_many(&[group, svg]).unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(
            0.0,
            gaanim_timeline::snapshot::WorldSnapshot::capture(&mut world),
        );
        timeline.seek(&mut world, 0.0);

        let memberships: Vec<_> = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .map(|(entity, _)| world.get::<SceneMember>(entity).copied())
            .collect();
        assert!(memberships.len() >= 7, "expected group and SVG hierarchies");
        assert!(memberships.iter().all(Option::is_none));
    }

    #[test]
    fn persistent_transform_does_not_localize_its_source() {
        let mut canvas = SceneModel::new(1280, 720);
        let source = canvas.test_title("Source");
        canvas.persist(&source).unwrap();
        canvas.wait(0.5);
        canvas
            .segment("target", Some(TransitionType::CrossFade { duration: 0.2 }))
            .unwrap();
        let target = canvas.test_title("Target");
        source.transform(&target).duration(0.5);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.seek(&mut world, 0.8);
        let source_entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == source.id).then_some(entity))
            .expect("source entity");
        assert_eq!(world.get::<SceneMember>(source_entity), None);
    }

    #[test]
    fn cross_segment_transform_does_not_retag_source_in_initial_snapshot() {
        let mut canvas = SceneModel::new(1280, 720);
        canvas.segment("first", None).unwrap();
        let circle = canvas.circle(40.0);
        let diamond = canvas.rect(80.0, 80.0);
        circle.transform(&diamond).duration(1.0);

        canvas
            .segment(
                "second",
                Some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.2 }),
            )
            .unwrap();
        let replacement = canvas.rect(120.0, 40.0);
        circle.replacement_transform(&replacement).duration(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let timeline = world.resource::<Timeline>();
        let first_scene = timeline
            .scenes
            .values()
            .find(|scene| scene.name == "first")
            .map(|scene| scene.id)
            .expect("first scene");
        let mut query = world.query::<(bevy::prelude::Entity, &MobjectId)>();
        let circle_entity = query
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == circle.id).then_some(entity))
            .expect("circle entity");

        assert_eq!(
            world
                .get::<SceneMember>(circle_entity)
                .map(|member| member.0),
            Some(first_scene)
        );
    }

    #[test]
    fn tracker_arc_compiles_with_signal_and_regenerator() {
        let mut canvas = SceneModel::new(1280, 720);
        let tracker = canvas.value_tracker(0.4);
        let _arrow = canvas.always_redraw_arc(&tracker, 0.0, 0.0, 120.0, 0.0, 0.4, 1.0, 0.0);
        canvas.play(vec![tracker.animate_value_to(1.2).duration(1.0)]);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let tracker_entity = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::FloatSignal>>()
            .iter(&world)
            .next()
            .expect("tracker entity");
        let arrow_entity = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .iter(&world)
            .next()
            .expect("arrow entity");

        assert_eq!(
            world
                .get::<gaanim_animation::FloatSignal>(tracker_entity)
                .map(|signal| signal.value),
            Some(0.4)
        );
        assert!(
            world
                .get::<gaanim_animation::AlwaysRedrawRegen>(arrow_entity)
                .is_some()
        );

        world
            .get_mut::<gaanim_animation::FloatSignal>(tracker_entity)
            .expect("tracker signal")
            .value = 1.2;
        gaanim_animation::always_redraw_regen_system(&mut world);
        assert!(
            !world
                .get::<gaanim_scene::Path2D>(arrow_entity)
                .expect("reactive path")
                .0
                .elements()
                .is_empty(),
            "the regenerated arrow path must reflect the signal value"
        );
    }

    #[test]
    fn group_pivot_is_preserved_in_the_compiled_transform() {
        let mut canvas = SceneModel::new(1280, 720);
        let rail = canvas.line(40.0, 0.0, 140.0, 0.0);
        let _mechanism = canvas.group(&[&rail]).with_pivot(100.0, -18.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query::<(&MobjectId, &gaanim_math::SpatialTransform)>();
        let transform = query
            .iter(&world)
            .find_map(|(_, transform)| {
                (transform.anchor == gaanim_core::glam::DVec3::new(100.0, -18.0, 0.0))
                    .then_some(transform)
            })
            .expect("group transform");

        assert_eq!(
            transform.anchor,
            gaanim_core::glam::DVec3::new(100.0, -18.0, 0.0)
        );
    }

    #[test]
    fn axes_compile_grid_lines_and_ticks_with_independent_styles() {
        let mut canvas = SceneModel::new(640, 360);
        let axis_color = Color::from_rgb8(0x11, 0x22, 0x33);
        let grid_color = Color::from_rgb8(0x44, 0x55, 0x66);
        let tick_color = Color::from_rgb8(0x77, 0x88, 0x99);
        let config = crate::canvas::AxesConfig {
            y_grid: false,
            y_ticks: false,
            y_numbers: false,
            axis_color,
            grid_color,
            tick_color,
            x_label: Some("time".to_owned()),
            ..Default::default()
        };
        let _axes = canvas.axes((-200.0, 200.0, 50.0), (-100.0, 100.0, 25.0), config);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let mut query = world.query::<(&gaanim_scene::ObjectTag, &gaanim_scene::StrokeBrush)>();
        let layers = query
            .iter(&world)
            .filter_map(|(tag, stroke)| {
                let gaanim_core::peniko::Brush::Solid(color) = stroke.brush.as_ref()? else {
                    return None;
                };
                Some((tag.0.as_str(), *color, stroke.style.width))
            })
            .collect::<Vec<_>>();

        assert!(layers.contains(&("SvgPath#AxesLines", axis_color, 0.03)));
        assert!(layers.contains(&("SvgPath#AxesGrid", grid_color, 0.01)));
        assert!(layers.contains(&("SvgPath#AxesTicks", tick_color, 0.02)));
    }

    #[test]
    fn reactive_spring_regenerates_a_helical_path() {
        let mut canvas = SceneModel::new(1280, 720);
        let _spring = canvas.spring_between_with_crossing(
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(-80.0, 0.0, 0.0)),
            crate::canvas::CanvasEndpoint::Static(gaanim_core::glam::DVec3::new(80.0, 0.0, 0.0)),
            5,
            14.0,
            0.7,
        );

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        let spring = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .iter(&world)
            .next()
            .expect("reactive spring entity");
        gaanim_animation::always_redraw_regen_system(&mut world);
        let path = world
            .get::<gaanim_scene::Path2D>(spring)
            .expect("spring path");
        assert!(
            path.0.elements().len() > 100,
            "spring must have enough samples for a smooth helical coil"
        );
    }

    #[test]
    fn spring_support_uses_local_geometry_instead_of_a_tracking_spring() {
        let mut canvas = SceneModel::new(640, 360);
        let _support = canvas.support_at(
            CanvasEndpoint::Static(DVec3::new(80.0, 45.0, 0.0)),
            "spring",
            DVec3::Y,
            48.0,
            70.0,
            None,
        );

        let state = canvas.state.lock().expect("canvas state poisoned");
        assert!(
            !state
                .active()
                .ops
                .iter()
                .any(|op| matches!(op, Op::AttachTrackingSpring { .. })),
            "a support spring must remain local to its support so timeline pauses and seeks cannot place it at the scene origin"
        );
    }

    #[test]
    fn fixed_support_uses_connection_plate_without_a_stem() {
        let mut canvas = SceneModel::new(640, 360);
        let support = canvas.support_at(
            CanvasEndpoint::Static(DVec3::ZERO),
            "fixed",
            DVec3::Y,
            48.0,
            70.0,
            None,
        );
        assert!(matches!(
            &support.body.spec.lock().expect("body spec").kind,
            SpawnKind::GroupNoCenter(children) if children.is_empty()
        ));
        let ground_children = match &support.ground.spec.lock().expect("ground spec").kind {
            SpawnKind::GroupNoCenter(children) => children.clone(),
            other => panic!("expected fixed-support ground group, got {other:?}"),
        };
        assert_eq!(
            ground_children.len(),
            1,
            "fixed support must contain one connection plate"
        );
    }

    #[test]
    fn anchored_bar_and_labeled_dimension_compile_the_reactive_contract() {
        let mut canvas = SceneModel::new(640, 360);
        let frame = canvas.rect(180.0, 80.0).move_to(20.0, 0.0);
        let left = frame.anchor_point(Anchor::TopLeft, DVec3::ZERO);
        let right = frame.anchor_point(Anchor::TopRight, DVec3::ZERO);
        let _bar = canvas.bar_between(
            CanvasEndpoint::Static(DVec3::new(-180.0, 120.0, 0.0)),
            left.into(),
            9.0,
        );
        let dimension = canvas
            .dimension_between_with_options(
                left.into(),
                right.into(),
                45.0,
                DimensionOptions {
                    label: Some("$W_f$".to_owned()),
                    show_value: true,
                    unit: Some("mm".to_owned()),
                    scale: 0.5,
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(
            dimension
                .label
                .as_ref()
                .and_then(DrawableHandle::text_spec)
                .and_then(|spec| spec.style.size),
            Some(0.48),
            "dimension labels use the theme's scene-unit size"
        );

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert!(
            world
                .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::TrackingLine>>()
                .iter(&world)
                .next()
                .is_some(),
            "bar must compile as a reactive tracking line"
        );
        assert!(
            world
                .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::EndpointDistance>>()
                .iter(&world)
                .next()
                .is_some(),
            "numeric dimension must compile a distance-backed signal"
        );
        assert!(
            world
                .query_filtered::<
                    bevy::prelude::Entity,
                    With<gaanim_animation::DimensionLabelPlacement>,
                >()
                .iter(&world)
                .next()
                .is_some(),
            "dimension annotation must keep a reactive placement binding"
        );
        gaanim_animation::always_redraw_regen_system(&mut world);
        let dimension_parts = world
            .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::AlwaysRedrawRegen>>()
            .iter(&world)
            .collect::<Vec<_>>();
        assert_eq!(
            dimension_parts.len(),
            2,
            "dimension geometry must use two reactive parts"
        );
        let close_counts = dimension_parts
            .iter()
            .map(|entity| {
                world
                    .get::<gaanim_scene::Path2D>(*entity)
                    .expect("dimension part path")
                    .0
                    .elements()
                    .iter()
                    .filter(|element| matches!(element, gaanim_core::kurbo::PathEl::ClosePath))
                    .count()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            close_counts.iter().sum::<usize>(),
            5,
            "dimension geometry must contain two extensions, one baseline, and two heads"
        );
        assert!(
            dimension_parts.iter().all(|entity| world
                .get::<gaanim_scene::FillBrush>(*entity)
                .and_then(|fill| fill.0.as_ref())
                .is_some()),
            "dimension silhouette must carry a fill brush"
        );
        assert!(dimension.label.is_some());
        assert!(dimension.number.is_some());
        assert!(dimension.unit.is_some());
    }

    #[test]
    fn explicit_dimension_value_uses_parameter_without_driving_geometry() {
        let mut canvas = SceneModel::new(640, 360);
        let value = canvas.parameter(12.0).unwrap();
        let dimension = canvas
            .dimension_between_with_options(
                CanvasEndpoint::Static(DVec3::new(-80.0, 0.0, 0.0)),
                CanvasEndpoint::Static(DVec3::new(80.0, 0.0, 0.0)),
                35.0,
                DimensionOptions {
                    value: Some(value.source()),
                    format: ".1f".to_owned(),
                    unit: Some("m".to_owned()),
                    scale: 99.0,
                    ..Default::default()
                },
            )
            .unwrap();
        assert!(dimension.number.is_some(), "value must imply show_value");

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        assert!(
            world
                .query_filtered::<bevy::prelude::Entity, With<gaanim_animation::EndpointDistance>>()
                .iter(&world)
                .next()
                .is_none(),
            "an explicit semantic value must not create a distance-backed signal"
        );
        let readout = world
            .query::<&gaanim_animation::ReactiveReadout>()
            .single(&world)
            .expect("dimension readout");
        assert_eq!(readout.last_text, "12.0");
        assert_eq!(readout.parameters.len(), 1);
        let (_, signal_entity) = readout.parameters[0];
        assert_eq!(
            world
                .get::<gaanim_animation::FloatSignal>(signal_entity)
                .expect("value parameter signal")
                .value,
            12.0
        );
        assert!(
            world
                .query_filtered::<
                    bevy::prelude::Entity,
                    With<gaanim_animation::DimensionLabelPlacement>,
                >()
                .iter(&world)
                .next()
                .is_some(),
            "the annotation placement must remain endpoint-driven"
        );
    }

    #[test]
    fn readout_part_spacing_is_in_scene_units() {
        let mut canvas = SceneModel::new(16.0, 9.0);
        canvas
            .dimension_between_with_options(
                CanvasEndpoint::Static(DVec3::new(-2.0, 0.0, 0.0)),
                CanvasEndpoint::Static(DVec3::new(2.0, 0.0, 0.0)),
                0.5,
                DimensionOptions {
                    label: Some("W".to_owned()),
                    show_value: true,
                    unit: Some("m".to_owned()),
                    ..Default::default()
                },
            )
            .unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();

        // The gap between label, "=", value and unit is a fraction of the text size.
        let layout = world
            .query::<&gaanim_animation::ReactiveReadoutLayout>()
            .single(&world)
            .expect("dimension readout layout");
        assert!(layout.spacing > 0.0 && layout.spacing < DEFAULT_REACTIVE_TEXT_SIZE * 0.5);
    }

    #[test]
    fn component_force_keeps_physical_readout_separate_from_visual_scale() {
        let mut canvas = SceneModel::new(640, 360);
        let fx = canvas.parameter(3.0).unwrap();
        let fy = canvas.parameter(4.0).unwrap();
        let force = canvas
            .force_from_components(
                CanvasEndpoint::Static(DVec3::new(20.0, -10.0, 0.0)),
                fx.source(),
                fy.source(),
                10.0,
                Some("$F$".to_owned()),
                true,
                ".1f".to_owned(),
                Some("N".to_owned()),
                14.0,
                None,
                None,
            )
            .unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        gaanim_animation::endpoint_distance_system(&mut world);
        gaanim_animation::tracking_line_system(&mut world);
        gaanim_animation::tracking_vector_head_system(&mut world);

        let (_, distance) = world
            .query::<(
                &gaanim_animation::FloatSignal,
                &gaanim_animation::EndpointDistance,
            )>()
            .iter(&world)
            .next()
            .expect("force magnitude signal");
        assert!(matches!(
            distance.to,
            gaanim_animation::TrackingEndpoint::Offset { .. }
        ));
        let value = world
            .query::<(
                &gaanim_animation::FloatSignal,
                &gaanim_animation::EndpointDistance,
            )>()
            .iter(&world)
            .next()
            .unwrap()
            .0
            .value;
        assert!((value - 5.0).abs() < 1e-9);
        assert!(force.number.is_some());
        assert!(force.unit.is_some());
    }

    #[test]
    fn segments_use_absolute_ranges_and_only_explicit_stops_pause() {
        let mut canvas = SceneModel::new(1280, 720);
        let intro = canvas
            .segment_with("intro", None, Some("Opening".to_string()), None)
            .unwrap();
        canvas.wait(1.0);
        canvas.stop(Some("reveal".to_string())).unwrap();
        canvas.wait(2.0);
        let details = canvas
            .segment_with("details", None, None, Some("comparison".to_string()))
            .unwrap();
        canvas.wait(0.5);

        let manifest = canvas.segment_manifest();
        assert_eq!(manifest.segments.len(), 2);
        assert_eq!(manifest.segments[0].id, intro.id());
        assert_eq!(manifest.segments[0].name, "intro");
        assert_eq!(manifest.segments[0].notes.as_deref(), Some("Opening"));
        assert_eq!(manifest.segments[0].start_time, 0.0);
        assert_eq!(manifest.segments[0].end_time, 3.0);
        assert_eq!(manifest.segments[0].stops[0].time, 1.0);
        assert_eq!(manifest.segments[1].id, details.id());
        assert_eq!(manifest.segments[1].start_time, 3.0);
        assert_eq!(manifest.segments[1].end_time, 3.5);
        assert_eq!(canvas.current_time(), 3.5);

        let state = canvas.state.lock().expect("canvas state poisoned");
        let stops = state
            .segments
            .iter()
            .flat_map(|segment| segment.ops.iter())
            .filter(|op| matches!(op, Op::Stop))
            .count();
        assert_eq!(stops, 1);
    }

    #[test]
    fn declared_audio_starts_only_when_played_at_the_absolute_scene_cursor() {
        let path =
            std::env::temp_dir().join(format!("gaanim-declared-audio-{}.wav", std::process::id()));
        std::fs::write(&path, b"fixture").unwrap();

        let mut canvas = SceneModel::new(1280, 720);
        canvas.segment("intro", None).unwrap();
        canvas.wait(2.0);
        canvas.segment("audio", None).unwrap();
        canvas.wait(1.0);
        let audio = canvas.audio(&path, Some(2.0), 0.5, 0.1, 0.2).unwrap();

        assert!(canvas.audio_tracks.is_empty(), "declaration must be inert");
        canvas.play_items(vec![audio.into()]).unwrap();

        assert_eq!(canvas.audio_tracks.len(), 1);
        assert_eq!(canvas.audio_tracks[0].start_time, 3.0);
        assert_eq!(canvas.current_time(), 5.0);
        let state = canvas.state.lock().expect("canvas state poisoned");
        assert!(matches!(
            state.active().ops[state.active().ops.len() - 2],
            Op::Play(_)
        ));
        assert!(matches!(state.active().ops.last(), Some(Op::Wait(2.0))));
        drop(state);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn open_ended_audio_play_does_not_extend_the_timeline() {
        let path = std::env::temp_dir().join(format!(
            "gaanim-background-audio-{}.wav",
            std::process::id()
        ));
        std::fs::write(&path, b"fixture").unwrap();

        let mut canvas = SceneModel::new(1280, 720);
        canvas.wait(1.25);
        let audio = canvas.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
        canvas.play_items(vec![audio.into()]).unwrap();

        assert_eq!(canvas.audio_tracks[0].start_time, 1.25);
        assert_eq!(canvas.current_time(), 1.25);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn composition_schedules_open_audio_without_blocking_and_rejects_stretch() {
        let path =
            std::env::temp_dir().join(format!("gaanim-composed-audio-{}.wav", std::process::id()));
        std::fs::write(&path, b"fixture").unwrap();
        let mut canvas = SceneModel::new(320, 180);
        let audio = canvas.audio(&path, None, 1.0, 0.0, 0.0).unwrap();
        let visual = canvas.circle(8.0).animate().fade_in().duration(0.5);
        let plan = Composition::sequence(
            vec![Composition::leaf(audio), Composition::leaf(visual)],
            0.0,
        )
        .unwrap();
        let schedule = plan.schedule(None).unwrap();
        assert_eq!(schedule.entries[0].duration, None);
        assert_eq!(schedule.entries[1].start, 0.0);
        assert_eq!(schedule.span, 0.5);
        assert!(matches!(
            plan.clone().stretch(2.0),
            Err(PlayError::StretchContainsMedia)
        ));
        canvas
            .play_composition_configured(plan, None, None)
            .unwrap();
        assert_eq!(canvas.current_time(), 0.5);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn declared_video_activates_frames_and_embedded_audio_at_one_cursor() {
        let path =
            std::env::temp_dir().join(format!("gaanim-declared-video-{}.mp4", std::process::id()));
        std::fs::write(&path, b"fixture").unwrap();

        let mut canvas = SceneModel::new(1280, 720);
        canvas.wait(1.5);
        let playback = gaanim_media::VideoPlayback {
            path: path.clone(),
            metadata: gaanim_media::VideoMetadata {
                width: 1,
                height: 1,
                duration: 4.0,
                fps: 30.0,
                has_audio: true,
            },
            scene_start: 0.0,
            source_offset: 0.0,
            source_duration: 4.0,
            looping: false,
            speed: 2.0,
            audio: true,
            volume: 0.75,
            last_frame: None,
            active: true,
            intervals: Vec::new(),
        };
        let poster = gaanim_core::peniko::ImageData {
            data: gaanim_core::peniko::Blob::from(vec![0, 0, 0, 255]),
            format: gaanim_core::peniko::ImageFormat::Rgba8,
            alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        };
        let view = ImageOptions::default().resolve(1, 1).unwrap();
        let drawable = canvas.spawn(SpawnKind::Video {
            poster,
            view,
            playback,
        });
        let video = VideoClip {
            drawable: drawable.clone(),
            state: canvas.state.clone(),
            duration: Some(2.0),
            audio: Some(AudioTrack::from_media(&path, 0.0, 0.0, 4.0, 2.0, false, 0.75).unwrap()),
            activated: Arc::new(AtomicBool::new(false)),
        };

        assert_eq!(
            canvas.play_items(vec![video.clone().into(), video.clone().into()]),
            Err(PlayError::VideoAlreadyActivated)
        );
        assert_eq!(canvas.current_time(), 1.5);
        assert!(canvas.audio_tracks.is_empty());
        canvas.play_items(vec![video.clone().into()]).unwrap();

        let spec = drawable.spec.lock().expect("object spec poisoned");
        let SpawnKind::Video { playback, .. } = &spec.kind else {
            panic!("expected video spawn kind");
        };
        assert_eq!(playback.scene_start, 1.5);
        drop(spec);
        assert_eq!(canvas.audio_tracks[0].start_time, 1.5);
        assert_eq!(canvas.current_time(), 3.5);
        assert_eq!(
            canvas.play_items(vec![video.into()]),
            Err(PlayError::VideoAlreadyActivated)
        );
        assert_eq!(canvas.audio_tracks.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    fn synthetic_video(canvas: &mut SceneModel) -> VideoClip {
        let poster = gaanim_core::peniko::ImageData {
            data: gaanim_core::peniko::Blob::from(vec![0; 400 * 200 * 4]),
            format: gaanim_core::peniko::ImageFormat::Rgba8,
            alpha_type: gaanim_core::peniko::ImageAlphaType::Alpha,
            width: 400,
            height: 200,
        };
        let options = ImageOptions {
            width: Some(8.0),
            height: Some(4.0),
            ..Default::default()
        };
        let view = options.resolve(400, 200).unwrap();
        let playback = gaanim_media::VideoPlayback {
            path: "unused.mp4".into(),
            metadata: gaanim_media::VideoMetadata {
                width: 400,
                height: 200,
                duration: 10.0,
                fps: 30.0,
                has_audio: false,
            },
            scene_start: 0.0,
            source_offset: 1.0,
            source_duration: 2.0,
            looping: true,
            speed: 2.0,
            audio: false,
            volume: 0.5,
            last_frame: None,
            active: true,
            intervals: Vec::new(),
        };
        let drawable = canvas.spawn(SpawnKind::Video {
            poster,
            view,
            playback,
        });
        drawable.spec.lock().unwrap().media_frame = Some(options.media_frame(view));
        VideoClip {
            drawable,
            state: canvas.state.clone(),
            duration: None,
            audio: None,
            activated: Arc::new(AtomicBool::new(false)),
        }
    }

    #[test]
    fn media_segments_are_finite_atomic_and_composable() {
        let mut scene = SceneModel::new(1280, 720);
        let video = synthetic_video(&mut scene);
        let a = video.segment(2.0, 5.0, None, None, None).unwrap();
        let b = video.segment(5.0, 8.0, Some(1.0), None, None).unwrap();
        assert_eq!(
            Composition::leaf(a.clone()).schedule(None).unwrap().span,
            1.5
        );
        assert!(matches!(
            Composition::leaf(a.clone()).stretch(4.0),
            Err(PlayError::StretchContainsMedia)
        ));
        assert_eq!(
            scene.play_items(vec![a.clone().into(), b.clone().into()]),
            Err(PlayError::OverlappingVideoSegments)
        );
        assert_eq!(scene.current_time(), 0.0);
        assert!(!a.consumed.load(Ordering::Acquire));
        let sequence = Composition::sequence(
            vec![Composition::leaf(a.clone()), Composition::leaf(b.clone())],
            1.0,
        )
        .unwrap();
        scene
            .play_composition_configured(sequence, None, None)
            .unwrap();
        assert_eq!(scene.current_time(), 5.5);
        assert_eq!(
            scene.play_items(vec![a.into()]),
            Err(PlayError::VideoSegmentConsumed)
        );
        assert_eq!(
            scene.play_items(vec![video.clone().into()]),
            Err(PlayError::MixedVideoPlayback)
        );
        scene
            .play_items(vec![
                video.segment(2.0, 5.0, None, None, None).unwrap().into(),
            ])
            .unwrap();
        let spec = video.drawable.spec.lock().unwrap();
        let SpawnKind::Video { playback, .. } = &spec.kind else {
            unreachable!()
        };
        assert_eq!(playback.intervals.len(), 3);
        assert_eq!(playback.intervals[1].scene_start, 2.5);
    }

    #[test]
    fn media_segments_validate_owner_mode_and_ranges() {
        let mut scene = SceneModel::new(1280, 720);
        let video = synthetic_video(&mut scene);
        for (start, end) in [(f64::NAN, 2.0), (-1.0, 2.0), (2.0, 2.0), (0.0, 11.0)] {
            assert!(video.segment(start, end, None, None, None).is_err());
        }
        assert!(video.segment(0.0, 2.0, Some(0.0), None, None).is_err());
        assert!(video.segment(0.0, 2.0, None, None, Some(f64::NAN)).is_err());
        let segment = video.segment(0.0, 2.0, None, None, None).unwrap();
        let mut other = SceneModel::new(1280, 720);
        assert_eq!(
            other.play_items(vec![segment.clone().into()]),
            Err(PlayError::ForeignVideo)
        );
        assert_eq!(
            scene.play_items(vec![segment.clone().into(), video.clone().into()]),
            Err(PlayError::MixedVideoPlayback)
        );
        assert_eq!(scene.current_time(), 0.0);
        scene.play_items(vec![video.into()]).unwrap();
        assert_eq!(
            scene.play_items(vec![segment.into()]),
            Err(PlayError::MixedVideoPlayback)
        );
    }

    #[test]
    fn media_framing_validates_and_preserves_birth_state() {
        let mut scene = SceneModel::new(1280, 720);
        let video = synthetic_video(&mut scene)
            .frame(8.0, 4.5, ImageFit::Cover)
            .unwrap();
        let image = video.drawable.clone();
        assert_eq!(image.source_width().unwrap(), 400);
        assert!(image.clone().frame(0.0, 1.0, ImageFit::Contain).is_err());
        assert!(image.clone().crop(0.5, 0.0, 1.0, 1.0, true).is_err());
        let birth = image.spec.lock().unwrap().media_frame.unwrap();
        scene.wait(1.0);
        let anim = image.animate().crop(0.25, 0.25, 0.5, 0.5, true).unwrap();
        scene.play_items(vec![anim.into()]).unwrap();
        assert_eq!(
            image.spec.lock().unwrap().media_frame.unwrap().crop,
            gaanim_core::kurbo::Rect::new(100.0, 50.0, 300.0, 150.0)
        );
        image
            .clone()
            .quality(gaanim_core::peniko::ImageQuality::High)
            .unwrap();
        assert_eq!(
            scene.state.lock().unwrap().frozen_spawn_specs[&image.id]
                .media_frame
                .unwrap(),
            birth
        );
        assert_eq!(image.spec.lock().unwrap().media_frame.unwrap().width, 8.0);
    }

    #[test]
    fn segments_validate_names_links_and_duplicate_stops() {
        let mut canvas = SceneModel::new(1280, 720);
        assert!(matches!(
            canvas.segment("  ", None),
            Err(SegmentError::EmptyName)
        ));

        let first = canvas.segment("first", None).unwrap();
        assert!(matches!(
            canvas.segment("FIRST", None),
            Err(SegmentError::DuplicateName { .. })
        ));
        canvas.wait(1.0);
        canvas.stop(None).unwrap();
        assert!(matches!(
            canvas.stop(None),
            Err(SegmentError::DuplicateStopTime { .. })
        ));
        let second = canvas.segment("second", None).unwrap();
        assert!(
            canvas
                .link(&first, &second, TransitionType::CrossFade { duration: 0.2 })
                .is_ok()
        );

        let mut foreign_canvas = SceneModel::new(1280, 720);
        let foreign = foreign_canvas.segment("foreign", None).unwrap();
        assert!(matches!(
            canvas.link(&foreign, &second, TransitionType::Cut),
            Err(SegmentError::ForeignSegment)
        ));

        let mut unicode_canvas = SceneModel::new(1280, 720);
        unicode_canvas.segment("ÁREA", None).unwrap();
        assert!(matches!(
            unicode_canvas.segment("área", None),
            Err(SegmentError::DuplicateName { .. })
        ));
    }

    #[test]
    fn segment_template_metadata_is_preserved() {
        let mut canvas = SceneModel::new(1000, 600);
        canvas
            .segment_with("content", None, None, Some("lecture".to_string()))
            .unwrap();
        let manifest = canvas.segment_manifest();
        assert_eq!(manifest.segments[0].template.as_deref(), Some("lecture"));
    }

    #[test]
    fn segment_backgrounds_compile_with_scene_fallback_and_terminal_stop_hold() {
        let scene_color = Color::from_rgb8(10, 20, 30);
        let segment_color = Color::from_rgb8(40, 50, 60);
        let mut canvas = SceneModel::new(640, 360);
        canvas.set_background(Some(scene_color));
        canvas
            .segment_with_background(
                "intro",
                None,
                None,
                None,
                Some(gaanim_renderer::background::BackgroundPaint::solid(
                    segment_color,
                )),
            )
            .unwrap();
        canvas.wait(1.0);
        canvas.stop(None).unwrap();
        canvas.segment("details", None).unwrap();
        canvas.wait(1.0);

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let background = world
            .get_resource::<gaanim_renderer::pipeline::CanvasBackground>()
            .expect("compiled canvas background");

        assert_eq!(background.paint_at(0.5).fallback_color(), segment_color);
        assert_eq!(background.paint_at(1.0).fallback_color(), segment_color);
        assert_eq!(background.paint_at(1.1).fallback_color(), scene_color);
    }

    #[test]
    fn segment_post_processes_compile_with_scene_inheritance() {
        use gaanim_renderer::post_process::{PostProcessOverride, PostProcessShader};
        let shader = |scale: &str| {
            PostProcessShader::new(format!(
                "fn gaanim_post(uv: vec2<f32>, resolution: vec2<f32>, time: f32) -> vec4<f32> {{\n\
                 return gaanim_scene(uv) * {scale};\n}}"
            ))
            .unwrap()
        };
        let mut canvas = SceneModel::new(640, 360);
        canvas.set_post_process(Some(shader("0.5")));
        canvas.segment("inherits", None).unwrap();
        canvas.wait(1.0);
        let plain = canvas.segment("plain", None).unwrap();
        canvas
            .set_segment_post_process(&plain, PostProcessOverride::Disabled)
            .unwrap();
        canvas.wait(1.0);
        let custom = canvas.segment("custom", None).unwrap();
        canvas
            .set_segment_post_process(&custom, PostProcessOverride::Shader(shader("0.25")))
            .unwrap();
        canvas.wait(1.0);
        let foreign = SceneModel::new(640, 360).segment("other", None).unwrap();
        assert!(matches!(
            canvas.set_segment_post_process(&foreign, PostProcessOverride::Disabled),
            Err(SegmentError::ForeignSegment)
        ));

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let post = world
            .get_resource::<gaanim_renderer::post_process::CanvasPostProcess>()
            .expect("compiled canvas post-process");

        let source_at = |time| {
            post.shader_at(time)
                .map(|shader| shader.source().to_owned())
        };
        assert!(source_at(0.5).unwrap().contains("* 0.5"));
        assert_eq!(source_at(1.5), None);
        assert!(source_at(2.5).unwrap().contains("* 0.25"));
    }

    #[test]
    fn segment_layout_aliases_and_branding_are_reusable() {
        let mut canvas = SceneModel::new(1280, 720);
        canvas.set_branding(PresentationBrand {
            footer: Some("Slides · Research Lab".to_owned()),
            show_on_cover: false,
            ..Default::default()
        });
        let before = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        canvas
            .segment_with("cover", None, None, Some("title_slide".to_string()))
            .expect("cover segment");
        let after_cover = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(after_cover, before, "cover branding is opt-in");

        canvas.wait(0.1);
        canvas
            .segment_with("content", None, None, Some("lecture".to_string()))
            .expect("content segment");
        let after_content = canvas
            .state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .ops
            .len();
        assert_eq!(
            after_content, 2,
            "rule and numbered footer are added to the active segment"
        );
    }

    #[test]
    fn required_constraint_conflicts_fail_during_authoring() {
        let mut canvas = SceneModel::new(320, 180);
        let object = canvas.circle(10.0);
        let left = gaanim_layout::LayoutExpression::variable(
            gaanim_layout::LayoutId(object.id.as_raw()),
            gaanim_layout::LayoutAttribute::Left,
        );
        let result = canvas.constrain_layout(
            vec![
                gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
                gaanim_layout::LayoutConstraint::equal(left, 20.0.into()),
            ],
            None,
        );
        assert!(matches!(
            result,
            Err(SceneObjectError::Layout(
                gaanim_layout::LayoutError::Unsatisfiable(_)
            ))
        ));
    }

    #[test]
    fn weak_constraint_diagnostics_are_available_before_compile() {
        let mut canvas = SceneModel::new(320, 180);
        let object = canvas.circle(10.0);
        let left = gaanim_layout::LayoutExpression::variable(
            gaanim_layout::LayoutId(object.id.as_raw()),
            gaanim_layout::LayoutAttribute::Left,
        );
        canvas
            .constrain_layout(
                vec![
                    gaanim_layout::LayoutConstraint::equal(left.clone(), 10.0.into()),
                    gaanim_layout::LayoutConstraint::equal(left, 20.0.into())
                        .with_strength(gaanim_layout::ConstraintStrength::Weak),
                ],
                None,
            )
            .unwrap();
        let diagnostics = canvas.check_layout();
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("residual 10.000000"));

        let mut foreign = SceneModel::new(320, 180);
        let foreign_object = foreign.circle(10.0);
        assert!(!canvas.owns_drawable(&foreign_object));
    }

    #[test]
    fn layout_ownership_rejects_positioning_but_allows_visual_transforms() {
        let mut canvas = SceneModel::new(320, 180);
        let owner = canvas.group(&[]);
        let positioned = canvas.circle(10.0).move_to(12.0, 0.0);
        assert_eq!(
            positioned.claim_layout(&owner),
            Err(crate::canvas::LayoutOwnershipError::PositionalOperation)
        );

        let animated = canvas.circle(10.0);
        let _description = animated.animate().shift_by(8.0, 0.0);
        assert!(animated.claim_layout(&owner).is_ok());

        let visual = canvas.circle(10.0);
        let _ = visual.animate().scale_by(1.5).rotate_by(0.25);
        assert!(visual.claim_layout(&owner).is_ok());

        let mut foreign_canvas = SceneModel::new(320, 180);
        let foreign = foreign_canvas.circle(10.0);
        assert_eq!(
            foreign.claim_layout(&owner),
            Err(crate::canvas::LayoutOwnershipError::ForeignScene)
        );
    }

    fn leaf_opacities_at(canvas: &SceneModel, time: f64) -> Vec<f32> {
        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
        app.update();
        app.update();
        let mut opacities: Vec<f32> = app
            .world_mut()
            .query::<(&gaanim_scene::FillBrush, &gaanim_scene::GlobalOpacity)>()
            .iter(app.world())
            .filter(|(fill, _)| fill.0.is_some())
            .map(|(_, opacity)| opacity.0)
            .collect();
        opacities.sort_by(f32::total_cmp);
        opacities
    }

    #[test]
    fn group_opacity_is_one_multiplier_for_setters_and_animations() {
        // A group hidden immediately must reappear when its opacity animates.
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(20.0).fill(Color::WHITE);
        let group = canvas.group(&[&dot]).opacity(0.0);
        canvas.play(vec![group.animate().opacity(1.0).duration(1.0)]);
        assert_eq!(leaf_opacities_at(&canvas, 0.0), vec![0.0]);
        assert_eq!(leaf_opacities_at(&canvas, 1.0), vec![1.0]);

        // A nested group applies its opacity once, not once per level.
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(20.0).fill(Color::WHITE);
        let inner = canvas.group(&[&dot]);
        canvas.group(&[&inner]).opacity(0.5);
        canvas.wait(0.5);
        assert_eq!(leaf_opacities_at(&canvas, 0.5), vec![0.5]);

        // Immediate cuts after the declaration freezes follow the same rule.
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(20.0).fill(Color::WHITE);
        let group = canvas.group(&[&dot]);
        canvas.wait(0.5);
        let group = group.opacity(0.0);
        canvas.play(vec![group.animate().opacity(0.4).duration(0.5)]);
        assert_eq!(leaf_opacities_at(&canvas, 0.5), vec![0.0]);
        let opacity = leaf_opacities_at(&canvas, 1.0)[0];
        assert!((opacity - 0.4).abs() < 1e-5, "{opacity}");
    }

    #[test]
    fn headless_derived_geometry_resolves_fill_level_and_outline() {
        let mut canvas = SceneModel::new(320, 180);
        canvas.text("Vector fill level");
        let mask = canvas
            .circle(40.0)
            .no_fill()
            .stroke(Color::WHITE, 3.0)
            .move_to(0.0, -10.0)
            .opacity(0.0);
        let fill = canvas
            .fill_level(
                &mask,
                Brush::Solid(Color::from_rgb8(56, 189, 248)),
                0.0,
                FillLevelDirection::Up,
                true,
            )
            .unwrap();
        canvas.play(vec![fill.animate().fill_level(0.75).duration(1.2)]);

        let mut app = bevy::prelude::App::new();
        app.add_plugins(bevy::prelude::MinimalPlugins)
            .add_plugins(gaanim_scene::GaanimScenePlugin)
            .add_plugins(gaanim_animation::GaanimAnimationPlugin)
            .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
            .add_plugins(gaanim_text::GaanimTextPlugin)
            .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
        canvas.compile(app.world_mut());
        app.finish();
        app.cleanup();
        app.update();
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(1.2);
        app.update();

        let fill_paths = app
            .world_mut()
            .query_filtered::<(
                &gaanim_scene::Path2D,
                &gaanim_renderer::effects::FillLevelBinding,
                Option<&gaanim_scene::FillBrush>,
                Option<&gaanim_scene::GlobalOpacity>,
                Option<&gaanim_scene::WorldBounds>,
                Option<&gaanim_scene::Visible>,
            ), ()>()
            .iter(app.world())
            .map(|(path, binding, fill, opacity, bounds, visible)| {
                (
                    path.0.is_empty(),
                    binding.sources.len(),
                    fill.is_some_and(|fill| fill.0.is_some()),
                    opacity.map(|opacity| opacity.0),
                    bounds.map(|bounds| bounds.0.width()),
                    visible.is_some(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(fill_paths.len(), 1);
        assert_eq!(fill_paths[0].0, false);
        assert_eq!(fill_paths[0].1, 1);
        assert_eq!(fill_paths[0].2, true);
        assert_eq!(fill_paths[0].3, Some(1.0));
        assert_eq!(fill_paths[0].5, true);
        assert!(fill_paths[0].4.is_none());

        let outline_paths = app
            .world_mut()
            .query_filtered::<(
                &gaanim_scene::Path2D,
                &gaanim_renderer::effects::VectorOutlineBinding,
                Option<&gaanim_scene::StrokeBrush>,
                Option<&gaanim_scene::GlobalOpacity>,
                Option<&gaanim_scene::WorldBounds>,
                Option<&gaanim_scene::Visible>,
            ), ()>()
            .iter(app.world())
            .map(|(path, binding, stroke, opacity, bounds, visible)| {
                (
                    path.0.is_empty(),
                    binding.sources.len(),
                    stroke.is_some_and(|stroke| stroke.brush.is_some()),
                    opacity.map(|opacity| opacity.0),
                    bounds.map(|bounds| bounds.0.width()),
                    visible.is_some(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(outline_paths.len(), 1);
        assert_eq!(outline_paths[0].0, false);
        assert_eq!(outline_paths[0].1, 1);
        assert_eq!(outline_paths[0].2, true);
        assert_eq!(outline_paths[0].3, Some(1.0));
        assert_eq!(outline_paths[0].5, true);
        assert!(outline_paths[0].4.is_none());
    }

    #[test]
    fn custom_animation_composes_and_preserves_exact_seek_and_following_baselines() {
        use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(10.0).move_to(4.0, 0.0);
        let callback = CustomAnimation::new(vec![CustomChannel::Position], |alpha| {
            Ok(CustomValues {
                position: Some(DVec3::new(4.0 + alpha * alpha * 100.0, 0.0, 0.0)),
                ..Default::default()
            })
        })
        .unwrap();
        let custom = dot
            .animate()
            .custom(callback)
            .unwrap()
            .duration(2.0)
            .rate_func(RateFunc::Linear);
        let opacity = dot
            .animate()
            .opacity(0.5)
            .duration(2.0)
            .rate_func(RateFunc::Linear);
        canvas
            .play_items(vec![custom.into(), opacity.into()])
            .unwrap();
        canvas
            .play_items(vec![
                dot.animate()
                    .shift_by(10.0, 0.0)
                    .duration(1.0)
                    .rate_func(RateFunc::Linear)
                    .into(),
            ])
            .unwrap();

        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .next()
            .unwrap()
            .0;
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        for (time, x) in [
            (0.2469, 4.0 + 0.12345_f64.powi(2) * 100.0),
            (3.0, 114.0),
            (2.5, 109.0),
            (0.2469, 4.0 + 0.12345_f64.powi(2) * 100.0),
            (0.0, 4.0),
        ] {
            timeline.seek(&mut world, time);
            assert!(
                (world.get::<SpatialTransform>(entity).unwrap().translation.x - x).abs() < 1e-9
            );
        }
    }

    #[test]
    fn custom_animation_endpoint_uses_easing_before_following_relative_animation() {
        use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
        let mut canvas = SceneModel::new(320, 180);
        let dot = canvas.circle(10.0).move_to(4.0, 0.0);
        let callback = CustomAnimation::new(vec![CustomChannel::Position], |alpha| {
            Ok(CustomValues {
                position: Some(DVec3::new(4.0 + alpha * 100.0, 0.0, 0.0)),
                ..Default::default()
            })
        })
        .unwrap();
        let custom = dot
            .animate()
            .custom(callback)
            .unwrap()
            .duration(1.0)
            .rate_func(RateFunc::ThereAndBack);
        let after = dot
            .animate()
            .shift_by(10.0, 0.0)
            .duration(1.0)
            .rate_func(RateFunc::Linear);
        canvas
            .play_composition_configured(
                Composition::sequence(
                    vec![Composition::leaf(custom), Composition::leaf(after)],
                    0.0,
                )
                .unwrap(),
                None,
                None,
            )
            .unwrap();
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let entity = world
            .query::<(bevy::prelude::Entity, &MobjectId)>()
            .iter(&world)
            .next()
            .unwrap()
            .0;
        let mut timeline = world.remove_resource::<Timeline>().unwrap();
        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        for (time, x) in [(0.5, 104.0), (2.0, 14.0), (1.5, 9.0), (0.0, 4.0)] {
            timeline.seek(&mut world, time);
            assert!(
                (world.get::<SpatialTransform>(entity).unwrap().translation.x - x).abs() < 1e-9
            );
        }
    }

    #[test]
    fn custom_animation_rejects_conflicts_and_invalid_outputs_without_consuming() {
        use gaanim_animation::{CustomAnimation, CustomChannel, CustomValues};
        let mut scene = SceneModel::new(320, 180);
        let dot = scene.circle(10.0);
        let valid = CustomAnimation::new(vec![CustomChannel::Opacity], |alpha| {
            Ok(CustomValues {
                opacity: Some(alpha as f32),
                ..Default::default()
            })
        })
        .unwrap();
        let custom = dot.animate().custom(valid).unwrap();
        assert!(
            matches!(scene.play_items(vec![custom.clone().into(), dot.animate().opacity(0.2).into()]), Err(PlayError::ConflictingChannel { channel, .. }) if channel == "opacity")
        );
        assert_eq!(scene.current_time(), 0.0);
        scene.play_items(vec![custom.into()]).unwrap();
        let invalid =
            CustomAnimation::new(
                vec![CustomChannel::Opacity],
                |_| Ok(CustomValues::default()),
            )
            .unwrap();
        let cursor = scene.current_time();
        assert!(matches!(
            scene.play_items(vec![dot.animate().custom(invalid).unwrap().into()]),
            Err(PlayError::CustomAnimation(_))
        ));
        assert_eq!(scene.current_time(), cursor);
    }

    #[test]
    fn animation_descriptions_are_pure_until_play() {
        let mut scene = SceneModel::new(320, 180);
        let dot = scene.circle(10.0);
        let before_ops = scene.state.lock().unwrap().active().ops.len();
        let anim = dot
            .animate()
            .shift_by(20.0, 0.0)
            .fill(Color::from_rgb8(255, 0, 0))
            .duration(0.75)
            .delay(0.25);
        assert_eq!(scene.current_time(), 0.0);
        assert_eq!(scene.state.lock().unwrap().active().ops.len(), before_ops);

        scene.play_items(vec![anim.into()]).unwrap();
        assert_eq!(scene.current_time(), 1.0);
        assert!(matches!(
            scene.state.lock().unwrap().active().ops.last(),
            Some(Op::Play(_))
        ));
    }

    #[test]
    fn play_rejects_reuse_duplicates_foreign_owners_and_channel_conflicts() {
        let mut scene = SceneModel::new(320, 180);
        let dot = scene.circle(10.0);
        let used = dot.animate().opacity(0.5);
        scene.play_items(vec![used.clone().into()]).unwrap();
        assert_eq!(
            scene.play_items(vec![used.into()]),
            Err(PlayError::AnimationAlreadyConsumed)
        );

        let duplicate = dot.animate().fill(Color::from_rgb8(255, 0, 0));
        assert_eq!(
            scene.play_items(vec![duplicate.clone().into(), duplicate.into()]),
            Err(PlayError::DuplicateAnimation)
        );

        let left = dot.animate().shift_by(10.0, 0.0);
        let right = dot.animate().move_to(20.0, 0.0);
        assert!(matches!(
            scene.play_items(vec![left.into(), right.into()]),
            Err(PlayError::ConflictingChannel { channel, .. }) if channel == "translation"
        ));

        let mut foreign_scene = SceneModel::new(320, 180);
        let foreign = foreign_scene.circle(10.0).animate().fade_in();
        assert_eq!(
            scene.play_items(vec![foreign.into()]),
            Err(PlayError::ForeignAnimation)
        );
    }

    #[test]
    fn immediate_setters_after_time_advance_record_zero_duration_cuts() {
        let mut scene = SceneModel::new(320, 180);
        let dot = scene.circle(10.0).move_to(0.0, 0.0).fill(Color::WHITE);
        scene.wait(1.0);
        let cursor = scene.current_time();
        let initial = scene
            .state
            .lock()
            .unwrap()
            .frozen_spawn_specs
            .get(&dot.id)
            .cloned()
            .unwrap();
        dot.clone()
            .shift_by(12.0, 0.0)
            .fill(Color::from_rgb8(255, 0, 0));
        let state = scene.state.lock().unwrap();
        assert_eq!(state.active().cursor, cursor);
        assert!(
            state
                .active()
                .ops
                .iter()
                .any(|op| matches!(op, Op::Immediate(_)))
        );
        assert_eq!(initial.fill, Some(Brush::Solid(Color::WHITE)));
    }

    #[test]
    fn composition_resolves_nested_spans_without_flattening_early() {
        let mut scene = SceneModel::new(320, 180);
        let a = scene.circle(10.0).animate().fade_in().duration(1.0);
        let b = scene.circle(10.0).animate().fade_in().duration(2.0);
        let c = scene.circle(10.0).animate().fade_in().duration(1.0);
        let group =
            Composition::parallel(vec![Composition::leaf(b), Composition::leaf(c)]).unwrap();
        let plan = Composition::sequence(vec![Composition::leaf(a), group], -0.25).unwrap();
        let schedule = plan.schedule(None).unwrap();
        assert_eq!(schedule.span, 2.75);
        assert_eq!(schedule.entries[0].start, 0.0);
        assert_eq!(schedule.entries[1].start, 0.75);
        assert_eq!(schedule.entries[2].start, 0.75);
        assert_eq!(scene.current_time(), 0.0);
    }

    // -- TM-05: labels, relative positions and scene markers --

    fn assert_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn fade(scene: &mut SceneModel, seconds: f64) -> Composition {
        Composition::leaf(scene.circle(10.0).animate().fade_in().duration(seconds))
    }

    fn at(position: &str) -> InsertPosition {
        InsertPosition::parse(position).unwrap()
    }

    #[test]
    fn composition_labels_resolve_gsap_style_positions() {
        let mut scene = SceneModel::new(320, 180);
        let title = fade(&mut scene, 0.8);
        let subtitle = fade(&mut scene, 0.4);
        let plan = Composition::sequence(
            vec![title, Composition::label("golpe").unwrap(), subtitle],
            0.1,
        )
        .unwrap()
        .insert(fade(&mut scene, 1.0), at("golpe+0.15"))
        .unwrap()
        .insert(fade(&mut scene, 0.5), at("<"))
        .unwrap()
        .insert(fade(&mut scene, 0.2), at("-=0.3"))
        .unwrap()
        .insert(fade(&mut scene, 0.2), at(">+0.1"))
        .unwrap()
        .insert(fade(&mut scene, 0.2), at("<+=0.05"))
        .unwrap()
        .insert(fade(&mut scene, 0.2), at("+=0.5"))
        .unwrap()
        .insert(fade(&mut scene, 0.2), InsertPosition::at(0.25).unwrap())
        .unwrap();
        let schedule = plan.schedule(None).unwrap();
        // The label takes no gap: it marks where the subtitle starts.
        assert_eq!(schedule.labels.len(), 1);
        assert_eq!(schedule.labels[0].name, "golpe");
        assert_close(schedule.labels[0].time, 0.9);
        let starts: Vec<f64> = schedule.entries.iter().map(|entry| entry.start).collect();
        let expected = [0.0, 0.9, 1.05, 1.05, 1.75, 2.05, 2.1, 2.8, 0.25];
        for (actual, expected) in starts.iter().zip(expected) {
            assert_close(*actual, expected);
        }
        // Inserted items follow the children in path order.
        assert_eq!(schedule.entries[2].path, vec![3]);
        assert_close(schedule.span, 3.0);
        assert_eq!(scene.current_time(), 0.0);
    }

    #[test]
    fn composition_labels_nest_shift_and_stretch() {
        let mut scene = SceneModel::new(320, 180);
        let inner = Composition::sequence(
            vec![
                fade(&mut scene, 1.0),
                Composition::label("mid").unwrap(),
                fade(&mut scene, 1.0),
                Composition::label("tail").unwrap(),
            ],
            0.0,
        )
        .unwrap();
        let first = fade(&mut scene, 0.5);
        let plan = Composition::sequence(
            vec![Composition::label("start").unwrap(), first, inner],
            0.0,
        )
        .unwrap()
        .insert(Composition::label("late").unwrap(), at("tail-0.25"))
        .unwrap()
        .insert(fade(&mut scene, 0.5), at("late"))
        .unwrap()
        .delay(1.0)
        .unwrap();
        let labels = plan.schedule(None).unwrap().labels;
        let times: Vec<(&str, f64)> = labels
            .iter()
            .map(|label| (label.name.as_str(), label.time))
            .collect();
        assert_eq!(
            times,
            vec![("start", 1.0), ("mid", 2.5), ("late", 3.25), ("tail", 3.5)]
        );
        let stretched = Composition::parallel(vec![
            fade(&mut scene, 2.0),
            Composition::label("half").unwrap().delay(1.0).unwrap(),
        ])
        .unwrap()
        .stretch(4.0)
        .unwrap()
        .repeat(2, 0.0)
        .unwrap();
        let schedule = stretched.schedule(None).unwrap();
        assert_eq!(schedule.labels.len(), 1);
        assert_close(schedule.labels[0].time, 2.0);
        assert_close(schedule.span, 8.0);
    }

    #[test]
    fn composition_label_errors_are_clear() {
        let mut scene = SceneModel::new(320, 180);
        assert!(matches!(
            Composition::label("  "),
            Err(PlayError::EmptyLabel)
        ));
        assert!(matches!(
            Composition::label("1.5"),
            Err(PlayError::InvalidLabel(_))
        ));
        for malformed in [
            "", "  ", "<x", ">+", "+=abc", "-=-1", "-1", "=a", "+0.2", "nan",
        ] {
            assert!(
                matches!(
                    InsertPosition::parse(malformed),
                    Err(PlayError::InvalidInsertPosition(_))
                ),
                "{malformed:?} should be rejected"
            );
        }
        assert!(InsertPosition::at(-0.1).is_err());

        let base = Composition::sequence(
            vec![fade(&mut scene, 1.0), Composition::label("hit").unwrap()],
            0.0,
        )
        .unwrap();
        let unknown = base
            .clone()
            .insert(fade(&mut scene, 1.0), at("missing+0.2"))
            .unwrap();
        let error = unknown.schedule(None).unwrap_err();
        assert_eq!(
            error,
            PlayError::UnknownLabel {
                name: "missing".into(),
                known: "\"hit\"".into()
            }
        );
        assert!(error.to_string().contains("unknown label \"missing\""));
        let before_start = base
            .clone()
            .insert(fade(&mut scene, 1.0), at("hit-2"))
            .unwrap();
        assert!(matches!(
            before_start.schedule(None),
            Err(PlayError::InvalidInsertPosition(message)) if message.contains("before the composition start")
        ));
        let duplicate =
            Composition::parallel(vec![base.clone(), Composition::label("hit").unwrap()]).unwrap();
        assert_eq!(
            duplicate.schedule(None).unwrap_err(),
            PlayError::DuplicateLabel("hit".into())
        );
    }

    #[test]
    fn composition_exact_label_names_win_over_offsets() {
        let mut scene = SceneModel::new(320, 180);
        let plan = Composition::sequence(
            vec![
                fade(&mut scene, 1.0),
                Composition::label("part-2").unwrap(),
                fade(&mut scene, 1.0),
                Composition::label("intro-end").unwrap(),
            ],
            0.0,
        )
        .unwrap()
        .insert(fade(&mut scene, 0.1), at("part-2"))
        .unwrap()
        .insert(fade(&mut scene, 0.1), at("part-2+0.5"))
        .unwrap()
        .insert(fade(&mut scene, 0.1), at("intro-end-=0.25"))
        .unwrap();
        let starts: Vec<f64> = plan
            .schedule(None)
            .unwrap()
            .entries
            .iter()
            .map(|entry| entry.start)
            .collect();
        for (actual, expected) in starts.iter().zip([0.0, 1.0, 1.0, 1.5, 1.75]) {
            assert_close(*actual, expected);
        }
    }

    #[test]
    fn composition_labels_play_like_their_resolved_schedule() {
        let mut scene = SceneModel::new(320, 180);
        let plan = Composition::sequence(
            vec![fade(&mut scene, 1.0), Composition::label("hit").unwrap()],
            0.0,
        )
        .unwrap()
        .insert(fade(&mut scene, 0.5), at("hit+0.5"))
        .unwrap();
        let span = plan.schedule(None).unwrap().span;
        scene.play_composition_configured(plan, None, None).unwrap();
        assert_close(span, 2.0);
        assert_close(scene.current_time(), 2.0);
    }

    #[test]
    fn scene_markers_are_absolute_unique_and_segment_aware() {
        let mut scene = SceneModel::new(320, 180);
        scene.marker("intro").unwrap();
        scene.segment("first", None).unwrap();
        scene.wait(1.5);
        scene.marker(" climax ").unwrap();
        scene.segment("second", None).unwrap();
        scene.wait(0.5);
        scene.marker("fin").unwrap();
        let markers = scene.markers();
        let summary: Vec<(&str, f64, &str)> = markers
            .iter()
            .map(|marker| (marker.name.as_str(), marker.time, marker.segment.as_str()))
            .collect();
        assert_eq!(
            summary,
            vec![
                ("intro", 0.0, "first"),
                ("climax", 1.5, "first"),
                ("fin", 2.0, "second")
            ]
        );
        assert_eq!(scene.marker_time("fin"), Some(2.0));
        assert_eq!(scene.marker_time("missing"), None);
        assert!(matches!(
            scene.marker("climax"),
            Err(SegmentError::DuplicateMarker { .. })
        ));
        assert!(matches!(
            scene.marker("12.5"),
            Err(SegmentError::NumericMarkerName { .. })
        ));
        assert!(matches!(
            scene.marker(""),
            Err(SegmentError::EmptyMarkerName)
        ));
        // Markers are metadata: they neither move the cursor nor add stops.
        assert_eq!(scene.current_time(), 2.0);
        assert!(
            scene
                .segment_manifest()
                .segments
                .iter()
                .all(|segment| segment.stops.is_empty())
        );
    }

    #[test]
    fn composition_allows_sequential_channel_reuse_but_rejects_overlap() {
        let mut scene = SceneModel::new(320, 180);
        let dot = scene.circle(10.0);
        let sequence = Composition::sequence(
            vec![
                Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
                Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
            ],
            0.0,
        )
        .unwrap();
        scene
            .play_composition_configured(sequence, None, None)
            .unwrap();
        assert_eq!(scene.current_time(), 2.0);

        let overlap = Composition::sequence(
            vec![
                Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
                Composition::leaf(dot.animate().shift_by(10.0, 0.0)),
            ],
            -0.25,
        )
        .unwrap();
        assert!(matches!(
            scene.play_composition_configured(overlap, None, None),
            Err(PlayError::ConflictingChannel { channel, .. }) if channel == "translation"
        ));
    }

    #[test]
    fn composition_defaults_and_stretch_resolve_before_scheduling() {
        let mut scene = SceneModel::new(320, 180);
        let first = scene.circle(10.0).animate().fade_in();
        let second = scene.circle(10.0).animate().fade_in().duration(2.0);
        let plan = Composition::stagger(
            vec![Composition::leaf(first), Composition::leaf(second)],
            0.5,
        )
        .unwrap()
        .defaults(Some(0.75), Some(RateFunc::Linear))
        .unwrap()
        .stretch(5.0)
        .unwrap();
        let schedule = plan.schedule(None).unwrap();
        assert_eq!(schedule.span, 5.0);
        assert_eq!(schedule.entries[0].duration, Some(1.5));
        assert_eq!(schedule.entries[1].start, 1.0);
        assert_eq!(schedule.entries[1].duration, Some(4.0));
    }
}

#[cfg(test)]
mod scene_unit_default_tests {
    //! Defaults once authored in pixels must stay proportional to the 16 x 9
    //! logical frame (pixel values / 100, like the migrated text roles).
    use super::*;
    use crate::canvas::ops::Op;
    use crate::canvas::visualization::{ArrowFieldOptions, FlowParticleOptions, StreamLinesStyle};
    use gaanim_core::glam::DVec3;

    fn stroke_width(handle: &DrawableHandle) -> f64 {
        handle
            .spec
            .lock()
            .expect("object spec poisoned")
            .stroke
            .as_ref()
            .map(|(_, width)| *width)
            .expect("stroke")
    }

    fn fixed(x: f64, y: f64) -> CanvasEndpoint {
        CanvasEndpoint::Static(DVec3::new(x, y, 0.0))
    }

    #[test]
    fn point_on_curve_marker_is_a_small_dot() {
        let mut canvas = SceneModel::new(320, 180);
        let curve = canvas.line(-2.0, 0.0, 2.0, 0.0);
        let tracker = canvas.value_tracker(0.0);
        let marker = canvas.point_on_curve(&curve, &tracker);
        let spec = marker.spec.lock().expect("object spec poisoned");
        assert!(matches!(spec.kind, SpawnKind::Dot(radius) if radius == 0.08));
    }

    #[test]
    fn mechanics_annotations_use_scene_unit_strokes_and_heads() {
        let mut canvas = SceneModel::new(320, 180);
        let vector = canvas
            .vector_between_with_parts(
                fixed(-1.0, 0.0),
                fixed(1.0, 0.5),
                None,
                false,
                ".1f".to_owned(),
                None,
                1.0,
                0.14,
                None,
                None,
            )
            .expect("vector");
        assert_eq!(stroke_width(&vector.shaft), 0.04);
        let state = canvas.state.lock().expect("canvas state poisoned");
        let head = state.active().ops.iter().find_map(|op| match op {
            Op::AttachTrackingVectorHead { length, width, .. } => Some((*length, *width)),
            _ => None,
        });
        assert_eq!(head, Some((0.16, 0.12)));
        drop(state);

        let angle = canvas
            .angle_between_with_options(
                fixed(0.0, 0.0),
                CanvasRay::Direction(DVec3::X),
                CanvasRay::Direction(DVec3::Y),
                0.64,
                AngleDimensionOptions::default(),
            )
            .expect("angle");
        assert_eq!(stroke_width(&angle.arc), 0.03);
        assert_eq!(stroke_width(&angle.extensions), 0.02);

        let rack = canvas.rack(2.75, 18, None);
        assert!(stroke_width(&rack) <= 0.05, "{}", stroke_width(&rack));
    }

    #[test]
    fn option_defaults_are_in_scene_units() {
        assert_eq!(AngleDimensionOptions::default().label_gap, 0.12);
        let dimension = DimensionOptions::default();
        assert_eq!(
            (
                dimension.label_gap,
                dimension.line_width,
                dimension.dash_length,
                dimension.gap_length
            ),
            (0.10, 0.03, 0.12, 0.08)
        );
        let axes = crate::canvas::Axes3DConfig::default();
        assert_eq!(
            (axes.axis_width, axes.grid_width, axes.tick_width),
            (0.03, 0.01, 0.02)
        );
        assert_eq!(ArrowFieldOptions::default().width, 0.02);
        assert_eq!(ArrowFieldOptions::default().max_length, 0.28);
        assert_eq!(StreamLinesStyle::default().width, 0.02);
        assert_eq!(FlowParticleOptions::default().radius, 0.05);
    }
}

#[cfg(test)]
mod grow_arrow_tests {
    use super::*;
    use bevy::prelude::{Entity, World};
    use gaanim_core::kurbo::Shape;
    use gaanim_scene::MobjectId;
    use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
    use gaanim_timeline::snapshot::WorldSnapshot;
    use gaanim_timeline::timeline::Timeline;

    fn compile(canvas: &mut SceneModel) -> (World, Timeline) {
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(gaanim_text::font::FontRegistry::new());
        world.insert_resource(gaanim_text::prelude::TextConfig::default());
        canvas.compile(&mut world);
        world.flush();
        let timeline = world.remove_resource::<Timeline>().expect("timeline");
        (world, timeline)
    }

    fn arrow_grow_targets(timeline: &Timeline) -> Vec<gaanim_core::ObjectId> {
        timeline
            .clips
            .values()
            .filter_map(|clip| match &clip.payload {
                ClipPayload::Animation(animation)
                    if matches!(animation.lens, PropertyLensSpec::ArrowGrow { .. }) =>
                {
                    Some(animation.target)
                }
                _ => None,
            })
            .collect()
    }

    fn path_of(world: &World, entity: Entity) -> gaanim_core::kurbo::BezPath {
        (*world.get::<gaanim_scene::Path2D>(entity).expect("path").0).clone()
    }

    #[test]
    fn grow_arrow_extends_from_tail_and_restores_exact_arrow() {
        let mut canvas = SceneModel::new(320, 180);
        let arrow = canvas
            .arrow_with_dimensions((-2.0, 0.0), (2.0, 0.0), 0.4, 0.3, 0.08, None)
            .expect("arrow");
        canvas.play(vec![arrow.animate().grow_arrow().duration(1.0)]);

        let (mut world, mut timeline) = compile(&mut canvas);
        let targets = arrow_grow_targets(&timeline);
        assert_eq!(targets.len(), 1, "a solid arrow schedules one grow clip");
        let entity = world
            .query::<(Entity, &MobjectId)>()
            .iter(&world)
            .find_map(|(entity, id)| (id.0 == targets[0]).then_some(entity))
            .expect("arrow entity");
        let full = gaanim_math::ArrowShape::Straight {
            start: gaanim_core::kurbo::Point::new(-2.0, 0.0),
            end: gaanim_core::kurbo::Point::new(2.0, 0.0),
            head_length: 0.4,
            head_width: 0.3,
            body_width: 0.08,
        }
        .path();

        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        timeline.seek(&mut world, 0.0);
        assert!(path_of(&world, entity).elements().is_empty());

        timeline.seek(&mut world, 0.5);
        let mid = path_of(&world, entity).bounding_box();
        assert!((mid.x0 - -2.0).abs() < 1e-9, "tail stays anchored");
        assert!((mid.x1 - 0.0).abs() < 1e-9, "tip is halfway at t=0.5");
        assert!((mid.height() - 0.3).abs() < 1e-9, "head keeps its width");

        timeline.seek(&mut world, 1.0);
        assert_eq!(path_of(&world, entity), full);

        // Seeking backwards re-hides the arrow deterministically.
        timeline.seek(&mut world, 0.0);
        assert!(path_of(&world, entity).elements().is_empty());
    }

    #[test]
    fn grow_arrow_grows_a_connector_along_its_live_polyline() {
        let mut canvas = SceneModel::new(320, 180);
        let connector = canvas
            .connector(
                CanvasEndpoint::Static(DVec3::new(0.0, 0.0, 0.0)),
                CanvasEndpoint::Static(DVec3::new(2.0, 2.0, 0.0)),
                vec![CanvasEndpoint::Static(DVec3::new(2.0, 0.0, 0.0))],
                0.4,
                0.3,
                0.05,
                None,
            )
            .expect("connector");
        canvas.play(vec![
            connector
                .animate()
                .grow_arrow()
                .duration(1.0)
                .rate_func(RateFunc::Linear),
        ]);

        let (mut world, mut timeline) = compile(&mut canvas);
        assert!(arrow_grow_targets(&timeline).is_empty());
        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            ClipPayload::Animation(animation)
                if matches!(animation.lens, PropertyLensSpec::ConnectorGrow { .. })
        )));
        let entity = world
            .query_filtered::<Entity, bevy::prelude::With<gaanim_animation::updaters::TrackingConnector>>()
            .single(&world)
            .expect("connector entity");

        timeline.add_keyframe(0.0, WorldSnapshot::capture(&mut world));
        let mut bounds_at = |time: f64| {
            timeline.seek(&mut world, time);
            gaanim_animation::tracking_line_system(&mut world);
            path_of(&world, entity)
        };
        assert!(bounds_at(0.0).elements().is_empty(), "hidden before growing");
        let half = bounds_at(0.5).bounding_box();
        assert!((half.x1 - 2.0).abs() < 1e-9 && half.x0.abs() < 1e-9, "{half:?}");
        assert!((half.height() - 0.3).abs() < 1e-9, "the head keeps its width");
        let full = bounds_at(1.0).bounding_box();
        assert!((full.y1 - 2.0).abs() < 1e-9, "the tip reaches the endpoint");
        // Reverse seeks are exact.
        assert_eq!(bounds_at(0.5).bounding_box(), half);
        assert!(bounds_at(0.0).elements().is_empty());
    }

    #[test]
    fn grow_arrow_on_non_arrow_falls_back_to_create() {
        let mut canvas = SceneModel::new(320, 180);
        let circle = canvas.circle(1.0);
        canvas.play(vec![circle.animate().grow_arrow().duration(1.0)]);

        let (_world, timeline) = compile(&mut canvas);
        assert!(arrow_grow_targets(&timeline).is_empty());
        assert!(timeline.clips.values().any(|clip| matches!(
            &clip.payload,
            ClipPayload::Animation(animation)
                if matches!(animation.lens, PropertyLensSpec::PathCompletion { .. })
        )));
    }
}
