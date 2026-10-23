//! DrawableHandle — ergonomic mobject handle with fluent configuration.

use gaanim_animation::{PropertySources, ScalarSource};
use std::collections::HashMap;
use std::sync::Arc;

use gaanim_animation::{
    AxisMask, InvalidSampledSeries, SampledInterpolation, SampledProperty, SampledSeriesDriver,
};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec2, DVec3, EulerRot};
use gaanim_core::kurbo::BezPath;
use gaanim_core::peniko::{Brush, Color};
use gaanim_layout::{Anchor, Direction};
use gaanim_text::prelude::TextAnchor;

use crate::anim::{
    AnimationBuilder, AnimationType, DrawAnimationConfig, PropertyAnimation, TextSelectionEffect,
};
use crate::canvas::authored::Authored;
use crate::canvas::ops::{
    AnchorPoint, FragmentRevealStyle, Op, SharedCanvasState, SharedObjectSpec, UpdaterPreset,
};
use crate::canvas::types::{Anim, LayoutOp, ObjectSpec, OptDuration, SpawnKind};

/// An axis range: (min, max, step).
pub type AxisRange = (f64, f64, f64);

/// An ergonomic handle to a mobject on a SceneModel.
///
/// - Immediate setters return `Self` (fluent): `obj.move_to(0, 0).fill(RED)`.
/// - Animation methods return `Anim` and auto-enqueue the animation on the
///   active segment's sequential track. They accept an optional duration:
///     - `obj.fade_in()`       — default 1.0s
///     - `obj.fade_in(2.0)`    — 2.0s
///     - `obj.fade_in(None)`   — default 1.0s (explicit)
///
///   You can still chain `.duration()` or any other `Anim` method after.
#[derive(Debug, Clone)]
pub struct DrawableHandle {
    pub id: ObjectId,
    pub(crate) spec: SharedObjectSpec,
    pub(crate) state: SharedCanvasState,
    pub(crate) segment_idx: usize,
    named_parts: Option<Arc<HashMap<String, DrawableHandle>>>,
    /// The animations of an imported glTF model, on its root.
    gltf: Option<Arc<gaanim_objects::gltf::GltfAnimations>>,
    /// The morph targets of a glTF node's mesh, or of every node of a
    /// model on its root.
    morph_targets: Arc<[Arc<GltfMorphTargets>]>,
    style_targets: Arc<Vec<SharedObjectSpec>>,
}

/// The morph targets of a glTF node's mesh: their names, and the node's
/// primitives that have them, which take their weights.
#[derive(Debug)]
pub(crate) struct GltfMorphTargets {
    pub(crate) names: Arc<[String]>,
    pub(crate) primitives: Vec<DrawableHandle>,
}

impl GltfMorphTargets {
    /// These targets on the copies of their primitives in `copies`; `None`
    /// when none was copied.
    fn remapped(&self, copies: &HashMap<ObjectId, DrawableHandle>) -> Option<Arc<Self>> {
        let primitives: Vec<DrawableHandle> = self
            .primitives
            .iter()
            .filter_map(|primitive| copies.get(&primitive.id).cloned())
            .collect();
        (!primitives.is_empty()).then(|| {
            Arc::new(Self {
                names: Arc::clone(&self.names),
                primitives,
            })
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImagePixelError {
    #[error("pixel(), point_ref() and region() need an image or a video drawable")]
    NotImage,
    #[error("pixel coordinates must be finite")]
    NonFinite,
    #[error("a region needs a positive width and height")]
    EmptyRegion,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SvgPartError {
    #[error("this drawable has no named parts: only imported SVGs and glTF models do")]
    NotSvg,
    #[error("unknown part '{id}'; available parts: {available}")]
    Unknown { id: String, available: String },
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum MorphTargetError {
    #[error(
        "this drawable has no morph targets: only glTF models and nodes whose meshes have them do"
    )]
    NoTargets,
    #[error("unknown morph target '{name}'; available targets: {available}")]
    Unknown { name: String, available: String },
    #[error("a morph target's weight must be finite")]
    InvalidWeight,
}

/// The primitives among `targets` with a morph target named `name`, each
/// with its spec and the index of that target among its own.
pub(crate) fn morph_entries<'a>(
    targets: &'a [Arc<GltfMorphTargets>],
    name: &str,
    weight: f64,
) -> Result<Vec<(ObjectId, &'a SharedObjectSpec, usize)>, MorphTargetError> {
    if targets.is_empty() {
        return Err(MorphTargetError::NoTargets);
    }
    if !weight.is_finite() {
        return Err(MorphTargetError::InvalidWeight);
    }
    let entries: Vec<_> = targets
        .iter()
        .filter_map(|targets| {
            let index = targets.names.iter().position(|named| named == name)?;
            Some(
                targets
                    .primitives
                    .iter()
                    .map(move |primitive| (primitive.id, &primitive.spec, index)),
            )
        })
        .flatten()
        .collect();
    if entries.is_empty() {
        let mut available: Vec<&str> = Vec::new();
        for targets in targets {
            for named in targets.names.iter() {
                if !available.contains(&named.as_str()) {
                    available.push(named);
                }
            }
        }
        return Err(MorphTargetError::Unknown {
            name: name.to_owned(),
            available: available.join(", "),
        });
    }
    Ok(entries)
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GltfAnimationError {
    #[error("this drawable is not an imported glTF model, which alone has animations")]
    NotGltf,
    #[error("unknown glTF animation '{name}'; available animations: {available}")]
    Unknown { name: String, available: String },
    #[error("glTF animation speed must be finite and greater than zero")]
    InvalidSpeed,
    #[error("glTF animation duration, transition and start time must be finite and non-negative")]
    InvalidTiming,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid 3D rotation axis '{axis}'; expected x, y, or z")]
pub struct RotationAxisError {
    pub axis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown matching mode '{mode}'; expected \"shapes\", \"tex\" or \"text\"")]
pub struct MatchingModeError {
    pub mode: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("material operations require a native Primitive3D drawable")]
pub struct Primitive3DHandleError;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LayoutOwnershipError {
    #[error("drawable belongs to a different Scene")]
    ForeignScene,
    #[error("drawable already belongs to layout {owner:?}")]
    AlreadyManaged { owner: ObjectId },
    #[error(
        "layout owns this drawable's position; remove at/shift/next_to/align_to/to_edge/to_corner and use .item(offset=...) or absolute placement"
    )]
    PositionalOperation,
}

/// Options for a vector clipping relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipOptions {
    pub rule: gaanim_core::peniko::Fill,
    pub invert: bool,
}

impl Default for ClipOptions {
    fn default() -> Self {
        Self {
            rule: gaanim_core::peniko::Fill::NonZero,
            invert: false,
        }
    }
}

/// A wipe needs a direction of travel; a zero or non-finite vector has none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("wipe direction must be a finite, non-zero vector")]
pub struct InvalidWipe;

/// `orient_3d` needs two finite, non-zero axes that are not parallel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("orient_3d needs two finite, non-zero axes that are not parallel")]
pub struct InvalidOrientation;

/// Side of the square mask a wipe slides over its target, in scene units:
/// it reaches far past any target, so only its leading edge ever shows.
const WIPE_MASK_SIDE: f64 = 2000.0;

/// Clip `target` to a hidden square whose leading edge sweeps across the
/// target's bounds in `direction` as `reveal` goes from 0 to 1. The edge
/// runs from the bounds' corner (or side) behind the travel to the one
/// ahead of it, both read every frame, so the wipe follows the target.
pub(crate) fn wipe_mask(
    state: &SharedCanvasState,
    target: ObjectId,
    reveal: ScalarSource,
    direction: Direction,
) -> Result<DrawableHandle, InvalidWipe> {
    let travel = direction.to_vector().truncate();
    if !travel.is_finite() || travel.length_squared() <= f64::EPSILON {
        return Err(InvalidWipe);
    }
    let travel = travel.normalize();
    // The extreme point of the bounds along the travel: a corner, or the
    // middle of a side for a horizontal or vertical wipe.
    let sign = |component: f64| {
        if component.abs() < 1e-9 {
            0.0
        } else {
            component.signum()
        }
    };
    let ahead = DVec3::new(sign(travel.x), sign(travel.y), 0.0);
    let scene_id = state.lock().expect("canvas state poisoned").scene_id;
    let anchor = |normalized: DVec3| {
        crate::canvas::CanvasEndpoint::Anchor(AnchorPoint {
            scene_id,
            object: target,
            normalized,
            offset: DVec3::ZERO,
        })
    };
    let mask =
        super::canvas_impl::spawn_in(state, SpawnKind::Rect(WIPE_MASK_SIDE, WIPE_MASK_SIDE), true)
            .no_fill()
            .no_stroke()
            .rotate_to(travel.y.atan2(travel.x));
    // Until the follow below takes over at the cursor, the mask waits far
    // behind the travel, so the target is hidden rather than whole.
    let waiting = -travel * WIPE_MASK_SIDE * 10.0;
    let mask = mask.move_to(waiting.x, waiting.y);
    let mut state = state.lock().expect("canvas state poisoned");
    let ops = &mut state.active_mut().ops;
    // The square's center trails its leading edge by half a side.
    ops.push(Op::AttachEndpointFollow {
        target: mask.id,
        endpoint: crate::canvas::CanvasEndpoint::Between {
            // A little past the bounds, so antialiasing shows no sliver
            // at either end.
            from: Box::new(anchor(-ahead * 1.02)),
            to: Box::new(anchor(ahead * 1.02)),
            alpha: reveal,
            offset: (-travel * WIPE_MASK_SIDE * 0.5).extend(0.0),
        },
        offset: DVec3::ZERO,
        offset_space: gaanim_animation::FollowOffsetSpace::World,
    });
    ops.push(Op::SetClip {
        target,
        mask: Some(mask.id),
        rule: gaanim_core::peniko::Fill::NonZero,
        invert: false,
    });
    drop(state);
    Ok(mask)
}

/// A deferred glyph selection inside a text-like [`DrawableHandle`].
#[derive(Debug, Clone)]
pub struct FragmentSelection {
    pub(crate) target: ObjectId,
    pub(crate) fragment: String,
    pub(crate) occurrence: Option<usize>,
    state: SharedCanvasState,
    layout_owner: Option<ObjectId>,
}

/// Split ordinary mathematical source into display terms when an equation has
/// not declared semantic tags. Operators stand alone; adjacent non-operator
/// characters (for example `c^2` or `2x`) remain one term.
fn math_source_terms(source: &str) -> Vec<String> {
    let mut terms = Vec::new();
    let mut current = String::new();
    for ch in source.chars() {
        if ch.is_whitespace() {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
        } else if matches!(ch, '=' | '+' | '-' | '*' | '/' | '(' | ')' | '[' | ']') {
            if !current.is_empty() {
                terms.push(std::mem::take(&mut current));
            }
            terms.push(ch.to_string());
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        terms.push(current);
    }
    terms
}

/// Number of vertices `points()` sets on a drawable of `kind`.
pub(crate) fn declared_vertices(kind: &SpawnKind) -> Result<usize, String> {
    match kind {
        SpawnKind::Polygon(points) | SpawnKind::Polyline { points, .. } => Ok(points.len()),
        SpawnKind::ReactivePolyline { .. } => Err(
            "points() requires fixed coordinates; this polyline follows reactive values"
                .to_string(),
        ),
        _ => Err("points() requires a polygon or polyline".to_string()),
    }
}

#[allow(dead_code)]
impl DrawableHandle {
    /// Source pixel width for raster images and video.
    pub fn source_width(&self) -> Result<u32, &'static str> {
        super::types::media_dimensions(&self.spec.lock().expect("object spec poisoned"))
            .map(|size| size.0)
    }
    pub fn source_height(&self) -> Result<u32, &'static str> {
        super::types::media_dimensions(&self.spec.lock().expect("object spec poisoned"))
            .map(|size| size.1)
    }
    /// Configure a fixed destination frame in scene units.
    pub fn frame(
        self,
        width: f64,
        height: f64,
        fit: super::ImageFit,
    ) -> Result<Self, &'static str> {
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return Err("frame dimensions must be finite and positive");
        }
        self.update_media_frame(|mut frame| {
            frame.width = width;
            frame.height = height;
            frame.fit = fit;
            frame.alignment = 0.5;
            frame
        })
    }
    pub fn crop(
        self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        normalized: bool,
    ) -> Result<Self, &'static str> {
        let to = super::types::cropped_frame(
            &self.spec.lock().expect("object spec poisoned"),
            x,
            y,
            width,
            height,
            normalized,
        )?;
        self.update_media_frame(|_| to)
    }
    pub fn quality(self, quality: gaanim_core::peniko::ImageQuality) -> Result<Self, &'static str> {
        self.update_media_frame(|mut frame| {
            frame.quality = quality;
            frame
        })
    }
    fn update_media_frame(
        self,
        update: impl FnOnce(gaanim_scene::MediaFrame) -> gaanim_scene::MediaFrame,
    ) -> Result<Self, &'static str> {
        let (from, to) = {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let from = spec
                .media_frame
                .ok_or("media operation requires Image or Video")?;
            let to = update(from);
            spec.media_frame = Some(to);
            (from, to)
        };
        self.push_immediate(
            self.id,
            AnimationType::Properties(PropertyAnimation {
                media_frame: Some((from, to)),
                ..Default::default()
            }),
        );
        Ok(self)
    }

    pub fn bounds_target(&self) -> crate::anim::BoundsTarget {
        crate::anim::BoundsTarget::Drawable(self.id)
    }

    /// Return a non-rendered point bound to this drawable's local bounds.
    pub fn anchor_point(&self, anchor: Anchor, offset: DVec3) -> AnchorPoint {
        let normalized = anchor.to_offset();
        AnchorPoint {
            scene_id: self.state.lock().expect("canvas state poisoned").scene_id,
            object: self.id,
            normalized: DVec3::new(normalized.x, normalized.y, 0.0),
            offset,
        }
    }

    /// The point of an image or video at pixel `(px, py)` of its source file,
    /// measured from the top-left corner with y growing downwards, as image
    /// editors report it. Crops and display sizes are taken into account, and
    /// the point follows the drawable when it moves, scales or turns.
    pub fn pixel(&self, px: f64, py: f64) -> Result<AnchorPoint, ImagePixelError> {
        if !px.is_finite() || !py.is_finite() {
            return Err(ImagePixelError::NonFinite);
        }
        let view = match &self.spec.lock().expect("object spec poisoned").kind {
            SpawnKind::Image { view, .. } | SpawnKind::Video { view, .. } => *view,
            _ => return Err(ImagePixelError::NotImage),
        };
        // The same mapping that draws the image (see `primitives::image`):
        // top-left/Y-down pixels onto the centred, Y-up box.
        let local = DVec3::new(
            (px - view.source_x) * view.scale_x - view.display_width * 0.5,
            view.display_height * 0.5 - (py - view.source_y) * view.scale_y,
            0.0,
        );
        Ok(self.anchor_point(Anchor::Center, local))
    }

    /// Pixel `(x, y)` of an image's or a video's source file, from its
    /// top-left corner with y down, or as fractions of the source's size
    /// with `normalized`. Unlike [`Self::pixel`] the point is read every
    /// frame through the drawable's frame and its current crop, so it stays
    /// on the same detail while `animate.crop` pans or zooms, and it follows
    /// the drawable and its parents as they move, scale or turn.
    pub fn media_point(
        &self,
        x: f64,
        y: f64,
        normalized: bool,
    ) -> Result<super::PointRef, ImagePixelError> {
        let (x, y) = self.source_pixel(x, y, normalized)?;
        Ok(super::PointRef(super::CanvasEndpoint::MediaPixel {
            object: self.id,
            x,
            y,
        }))
    }

    /// An invisible drawable over the source pixels `x, y, width, height`
    /// of an image or video (from the top-left corner with y down, or
    /// fractions of the source with `normalized`), kept on them as
    /// [`Self::media_point`] keeps a point. It has no paint: frames,
    /// notations, spotlights and connectors take it as a target. A region
    /// that a crop leaves outside the visible frame lies outside it too.
    pub fn media_region(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        normalized: bool,
    ) -> Result<DrawableHandle, ImagePixelError> {
        let (x0, y0) = self.source_pixel(x, y, normalized)?;
        let (x1, y1) = self.source_pixel(x + width, y + height, normalized)?;
        if !(x1 > x0 && y1 > y0) {
            return Err(ImagePixelError::EmptyRegion);
        }
        let region = super::canvas_impl::spawn_in(&self.state, SpawnKind::SurroundingRect, true)
            .no_fill()
            .no_stroke();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachMediaRegion {
                target: region.id,
                media: self.id,
                rect: [x0, y0, x1, y1],
            });
        Ok(region)
    }

    /// `(x, y)` in source pixels of this image or video.
    fn source_pixel(
        &self,
        x: f64,
        y: f64,
        normalized: bool,
    ) -> Result<(f64, f64), ImagePixelError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(ImagePixelError::NonFinite);
        }
        let spec = self.spec.lock().expect("object spec poisoned");
        let (width, height) =
            super::types::media_dimensions(&spec).map_err(|_| ImagePixelError::NotImage)?;
        Ok(if normalized {
            (x * f64::from(width), y * f64::from(height))
        } else {
            (x, y)
        })
    }

    /// Define a unique named anchor; existing point references remain immutable.
    pub fn with_port(
        self,
        name: &str,
        anchor: Anchor,
        offset: DVec3,
    ) -> Result<Self, &'static str> {
        if name.trim().is_empty() || !offset.is_finite() {
            return Err("port name must be nonempty and its offset finite");
        }
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if spec.ports.contains_key(name) {
            return Err("port name already defined");
        }
        spec.ports.insert(name.to_owned(), (anchor, offset));
        drop(spec);
        Ok(self)
    }

    /// Resolve an immutable named local anchor, following the drawable's bounds.
    pub fn port(&self, name: &str) -> Result<AnchorPoint, &'static str> {
        let (anchor, offset) = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .ports
            .get(name)
            .copied()
            .ok_or("unknown port name")?;
        Ok(self.anchor_point(anchor, offset))
    }

    /// Whether a bounds point was created by this drawable's Scene.
    pub fn owns_anchor_point(&self, point: AnchorPoint) -> bool {
        self.state.lock().expect("canvas state poisoned").scene_id == point.scene_id
    }

    /// Returns the immutable structured authoring snapshot for unified text.
    pub fn text_spec(&self) -> Option<gaanim_text::prelude::TextSpec> {
        let spec = self.spec.lock().expect("object spec poisoned");
        match &spec.kind {
            SpawnKind::Text(text) => Some(text.clone()),
            _ => None,
        }
    }

    fn text_semantic_pairs(
        &self,
        target: &DrawableHandle,
        requested: Option<Vec<(String, String)>>,
    ) -> Vec<(String, Option<usize>, String, Option<usize>)> {
        let source_tags = self
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
        text_semantic_pairs(&source_tags, &target_tags, requested)
    }

    fn require_text_transition_target(
        &self,
        target: &DrawableHandle,
    ) -> Result<(), LayoutOwnershipError> {
        if !self.same_canvas(target) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        let source_owner = self.layout_owner();
        let target_owner = target.layout_owner();
        if (source_owner.is_some() || target_owner.is_some()) && source_owner != target_owner {
            return Err(LayoutOwnershipError::AlreadyManaged {
                owner: target_owner.or(source_owner).expect("one owner exists"),
            });
        }
        Ok(())
    }

    /// Name this drawable. Names are the default key of a magic move
    /// ([`DrawableHandle::magic_move_to`]); a blank name clears it.
    pub fn named(&self, name: impl Into<String>) -> Self {
        let name = name.into();
        let name = name.trim();
        self.update_spec(|spec| spec.name = (!name.is_empty()).then(|| name.to_string()))
    }

    /// The name given with [`DrawableHandle::named`], if any.
    pub fn name(&self) -> Option<String> {
        self.spec.lock().expect("object spec poisoned").name.clone()
    }

    /// The key this drawable answers to in a magic move: its name (or SVG
    /// `id`) for [`MagicMoveKey::Name`], its SVG `id` for
    /// [`MagicMoveKey::SvgId`].
    pub fn magic_move_key(&self, key: super::MagicMoveKey) -> Option<String> {
        super::magic_move::spec_key(&self.spec.lock().expect("object spec poisoned"), key)
    }

    /// Handle of another drawable of this canvas, for key callbacks.
    fn sibling_handle(&self, id: ObjectId) -> Option<Self> {
        let spec = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .object_specs
            .get(&id)?
            .clone();
        Some(Self {
            id,
            spec,
            state: self.state.clone(),
            segment_idx: self.segment_idx,
            named_parts: None,
            gltf: None,
            morph_targets: Arc::new([]),
            style_targets: Arc::new(Vec::new()),
        })
    }

    /// Keyed members of this drawable: the outermost descendants for which
    /// `key` returns a non-empty key, in depth-first declaration order.
    pub fn magic_move_members<E>(
        &self,
        mut key: impl FnMut(&Self) -> Result<Option<String>, E>,
    ) -> Result<Vec<(ObjectId, String)>, E> {
        super::magic_move::keyed_outermost(
            &[self.id],
            false,
            |id| {
                self.sibling_handle(id)
                    .map(|handle| {
                        super::magic_move::spec_children(
                            &handle.spec.lock().expect("object spec poisoned"),
                        )
                    })
                    .unwrap_or_default()
            },
            |id| match self.sibling_handle(id) {
                Some(handle) => key(&handle),
                None => Ok(None),
            },
        )
    }

    /// Keyed "magic move" into `target`: members of both drawables that
    /// share a key morph position, size, color and shape (a pair of groups
    /// also morphs their parts), the other members appear or disappear as
    /// `unmatched` says, and `target` takes over at the end. Without a
    /// `duration` it lasts a second, or `Scene.play`'s default duration.
    pub fn magic_move_to(
        &self,
        target: &DrawableHandle,
        key: super::MagicMoveKey,
        unmatched: super::MagicMoveUnmatched,
        duration: Option<f64>,
    ) -> Result<Anim, super::MagicMoveError> {
        self.magic_move_to_by(
            target,
            |member| Ok::<_, std::convert::Infallible>(member.magic_move_key(key)),
            unmatched,
            duration,
        )
        .map_err(|failure| match failure {
            super::MagicMoveFailure::Invalid(error) => error,
            super::MagicMoveFailure::Key(never) => match never {},
        })
    }

    /// [`DrawableHandle::magic_move_to`] with keys computed by `key`, which
    /// is called once per candidate member of both drawables while the
    /// scene is built. Returning `None` (or an empty key) descends into the
    /// member's own members.
    pub fn magic_move_to_by<E>(
        &self,
        target: &DrawableHandle,
        mut key: impl FnMut(&Self) -> Result<Option<String>, E>,
        unmatched: super::MagicMoveUnmatched,
        duration: Option<f64>,
    ) -> Result<Anim, super::MagicMoveFailure<E>> {
        if !self.same_canvas(target) {
            return Err(super::MagicMoveError::ForeignScene.into());
        }
        if self.id == target.id {
            return Err(super::MagicMoveError::SameDrawable.into());
        }
        if duration.is_some_and(|duration| !duration.is_finite() || duration <= 0.0) {
            return Err(super::MagicMoveError::InvalidDuration.into());
        }
        let sources = self
            .magic_move_members(&mut key)
            .map_err(super::MagicMoveFailure::Key)?;
        let targets = target
            .magic_move_members(&mut key)
            .map_err(super::MagicMoveFailure::Key)?;
        let pairs = super::pair_by_key(&sources, &targets).pairs;
        Ok(self.anim_dur(
            AnimationType::MagicMove {
                target: target.id,
                pairs,
                unmatched,
            },
            duration,
        ))
    }

    /// General text/math morph. Semantic paths are paired before the existing
    /// order-preserving grapheme and shape matching stages.
    pub fn morph_to(
        &self,
        target: &DrawableHandle,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self.text_semantic_pairs(target, None);
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Structured derivation step, replacing the equation-only scene method.
    pub fn step_to(
        &self,
        target: &DrawableHandle,
        matches: Option<Vec<(String, String)>>,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self.text_semantic_pairs(target, matches);
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Expands text around one semantic path while new graphemes enter.
    pub fn expand_to(
        &self,
        target: &DrawableHandle,
        anchor: &str,
        duration: f64,
    ) -> Result<Anim, LayoutOwnershipError> {
        self.require_text_transition_target(target)?;
        let semantic_pairs = self
            .text_semantic_pairs(target, Some(vec![(anchor.to_string(), anchor.to_string())]))
            .into_iter()
            .take(1)
            .collect();
        Ok(self.anim_dur(
            AnimationType::TextTransition {
                target: target.id,
                copy: false,
                semantic_pairs,
            },
            Some(duration),
        ))
    }

    /// Replace the authoring snapshot while preserving this handle identity.
    /// Timeline materialization observes the incremented version.
    /// Change the horizontal alignment of a Text object's lines in place,
    /// keeping its content, styles and fragment fills. Returns `false` for a
    /// drawable that is not text.
    pub fn set_text_align(&self, align: gaanim_text::prelude::TextAlign) -> bool {
        let mut spec = self.spec.lock().expect("object spec poisoned");
        let SpawnKind::Text(text) = &mut spec.kind else {
            return false;
        };
        text.flow.align = align;
        true
    }

    pub fn r#become(&self, text: gaanim_text::prelude::TextSpec, duration: Option<f64>) {
        fn set_text(spec: &mut ObjectSpec, text: gaanim_text::prelude::TextSpec) {
            let parts = text.parts();
            spec.kind = SpawnKind::Text(text);
            spec.fragment_tags.clear();
            spec.fragment_fills.clear();
            for part in parts {
                if let Some(color) = part.style.color {
                    spec.fragment_fills
                        .push((part.text.clone(), Some(part.occurrence), color));
                }
                spec.fragment_tags
                    .push((part.path.join("."), part.text, Some(part.occurrence)));
            }
        }

        let mut spec = self.spec.lock().expect("object spec poisoned");
        if !matches!(spec.kind, SpawnKind::Text(_)) {
            return;
        }
        let owner = spec.layout_owner;
        let frozen = self
            .state
            .lock()
            .expect("canvas state poisoned")
            .frozen_spawn_specs
            .contains_key(&self.id);
        if frozen && owner.is_none() {
            // The text has been shown and its declaration is frozen: a copy
            // with the new content takes its place and its identity at the
            // cursor, as after a replacing text transition.
            drop(spec);
            let copy = self.copy();
            set_text(&mut copy.spec.lock().expect("object spec poisoned"), text);
            let anim_type = AnimationType::TextTransition {
                target: copy.id,
                copy: false,
                semantic_pairs: Vec::new(),
            };
            let rate_func = anim_type.default_rate_func();
            self.state
                .lock()
                .expect("canvas state poisoned")
                .active_mut()
                .ops
                .push(Op::Animate {
                    anim: AnimationBuilder {
                        target: self.id,
                        anim_type,
                        duration: 0.0,
                        delay: 0.0,
                        rate_func,
                    },
                    active: true,
                });
            return;
        }
        set_text(&mut spec, text);
        drop(spec);

        let Some(owner) = owner else {
            return;
        };
        let mut state = self.state.lock().expect("canvas state poisoned");
        let mut affected = vec![owner];
        let mut index = 0;
        while index < affected.len() {
            let child = affected[index];
            for (root, snapshot) in &state.latest_layouts {
                if snapshot.members.iter().any(|member| member.id == child)
                    && !affected.contains(root)
                {
                    affected.push(*root);
                }
            }
            index += 1;
        }
        for root in affected {
            let Some(previous) = state.latest_layouts.get(&root).cloned() else {
                continue;
            };
            let mut next = previous.clone();
            next.version = next.version.saturating_add(1);
            state.latest_layouts.insert(root, next.clone());
            state.active_mut().ops.push(Op::LayoutTransition {
                from_version: Some(previous.version),
                to: next,
                duration: duration.filter(|value| value.is_finite() && *value > 0.0),
                entering: None,
                leaving: None,
                resolve: true,
                remeasure: None,
            });
        }
    }

    pub fn same_canvas(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.state, &other.state)
    }

    /// Live derived geometry is defined in source world-space and therefore
    /// intentionally has no independent layout/transform ownership.
    pub fn is_live_derived_geometry(&self) -> bool {
        matches!(
            self.spec.lock().expect("object spec poisoned").kind,
            SpawnKind::Boolean { live: true, .. } | SpawnKind::SurroundingRect
        )
    }

    /// Set or bind a normalized fill level. Fixed values end a binding reversibly.
    pub fn set_fill_level(self, level: impl Into<ScalarSource>) -> Result<Self, String> {
        let level = level.into();
        let Some(level) = level.constant_value() else {
            return self.bind_property(PropertySources::FillLevel(level));
        };
        if !level.is_finite() || !(0.0..=1.0).contains(&level) {
            return Err("fill level must be finite and between zero and one".into());
        }
        let mut spec = self.spec.lock().expect("object spec poisoned");
        let SpawnKind::FillLevel { level: current, .. } = &mut spec.kind else {
            return Err("set_fill_level() requires a Scene.fill_level drawable".into());
        };
        *current = level;
        spec.fill_level_cursor = Some(level);
        drop(spec);
        self.clear_property_binding(gaanim_animation::PropertyChannel::FillLevel);
        self.push_immediate(
            self.id,
            AnimationType::FillLevelTo {
                from: level,
                to: level,
            },
        );
        Ok(self)
    }

    pub fn claim_layout(&self, owner: &DrawableHandle) -> Result<(), LayoutOwnershipError> {
        if !self.same_canvas(owner) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        if self.is_live_derived_geometry() {
            return Err(LayoutOwnershipError::PositionalOperation);
        }
        // A drawable placed or moved before joining a box is simply placed by
        // the box from then on.
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if let Some(existing) = spec.layout_owner
            && existing != owner.id
        {
            return Err(LayoutOwnershipError::AlreadyManaged { owner: existing });
        }
        spec.layout_owner = Some(owner.id);
        Ok(())
    }

    /// Whether `other` is this very handle: the same drawable, spec and
    /// style targets (a fluent setter's result), not a part or a copy.
    pub fn is_same_handle(&self, other: &DrawableHandle) -> bool {
        self.id == other.id
            && Arc::ptr_eq(&self.spec, &other.spec)
            && Arc::ptr_eq(&self.style_targets, &other.style_targets)
            && match (&self.named_parts, &other.named_parts) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
    }

    pub fn release_layout(&self, owner: &DrawableHandle) {
        let mut spec = self.spec.lock().expect("object spec poisoned");
        if spec.layout_owner == Some(owner.id) {
            spec.layout_owner = None;
        }
    }
    pub(crate) fn new(
        id: ObjectId,
        kind: SpawnKind,
        state: SharedCanvasState,
        segment_idx: usize,
    ) -> Self {
        Self {
            id,
            spec: Arc::new(Authored::sharing(ObjectSpec::new(id, kind), &state)),
            state,
            segment_idx,
            named_parts: None,
            gltf: None,
            morph_targets: Arc::new([]),
            style_targets: Arc::new(Vec::new()),
        }
    }

    /// Show `separator` between the integer and fractional digits of a
    /// reactive readout (`,` also turns `,` grouping into `.`). Digits,
    /// signs, whitespace, and non-readout drawables are rejected.
    pub fn readout_decimal_separator(&self, separator: char) -> Result<Self, String> {
        if separator.is_ascii_digit()
            || separator.is_whitespace()
            || matches!(separator, '+' | '-' | 'e' | 'E' | '%')
        {
            return Err(format!("invalid decimal separator {separator:?}"));
        }
        let mut is_readout = false;
        self.update_spec(|spec| {
            if let SpawnKind::ReactiveReadout {
                decimal_separator, ..
            } = &mut spec.kind
            {
                *decimal_separator = separator;
                is_readout = true;
            }
        });
        if is_readout {
            Ok(self.clone())
        } else {
            Err("decimal separators apply only to reactive readouts".into())
        }
    }

    /// Keep room inside a layout for a reactive readout showing `value`,
    /// besides the values its parameters are set or animated to: a live
    /// count has no known maximum. Non-readout drawables are rejected.
    pub fn readout_reserve(&self, value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("reserve must be a finite number".into());
        }
        let mut is_readout = false;
        self.update_spec(|spec| {
            if let SpawnKind::ReactiveReadout { reserve, .. } = &mut spec.kind {
                *reserve = Some(value);
                is_readout = true;
            }
        });
        if is_readout {
            Ok(self.clone())
        } else {
            Err("reserve applies only to reactive readouts".into())
        }
    }

    fn update_spec(&self, f: impl FnOnce(&mut ObjectSpec)) -> Self {
        f(&mut self.spec.lock().expect("object spec poisoned"));
        self.clone()
    }

    /// Apply a paint or style change to this drawable and, for groups, to
    /// every member leaf, since paint lives on the leaves.
    fn update_style(&self, f: impl Fn(&mut ObjectSpec)) -> Self {
        self.restyle(
            std::iter::once(&self.spec).chain(self.style_targets.iter()),
            f,
        )
    }

    fn restyle<'a>(
        &self,
        targets: impl Iterator<Item = &'a SharedObjectSpec>,
        f: impl Fn(&mut ObjectSpec),
    ) -> Self {
        for target in targets {
            let (before, after) = {
                let mut spec = target.lock().expect("object spec poisoned");
                let before = spec.clone();
                f(&mut spec);
                (before, spec.clone())
            };
            let mut properties = PropertyAnimation::default();
            if before.opacity != after.opacity {
                properties.opacity = Some(after.opacity);
            }
            if before.fill != after.fill
                && let Some(paint) = after.fill.as_ref()
            {
                properties.fill = Some(paint.clone());
            }
            if before.stroke != after.stroke
                && let Some((paint, width)) = after.stroke.as_ref()
            {
                properties.stroke_color = Some(paint.clone());
                properties.stroke_width = Some(*width);
            }
            // Effects cut like paints once the declaration is frozen; `None`
            // removes one.
            if before.glow != after.glow {
                properties.glow = Some(after.glow.clone());
            }
            if before.blur != after.blur {
                properties.blur = Some(after.blur);
            }
            if before.shadow != after.shadow {
                properties.shadow = Some(after.shadow.clone());
            }
            if !properties.is_empty() {
                self.push_immediate(
                    target.lock().expect("object spec poisoned").id,
                    AnimationType::Properties(properties),
                );
            }
        }
        self.clone()
    }

    pub(super) fn push_immediate(&self, target: ObjectId, anim_type: AnimationType) {
        let mut state = self.state.lock().expect("canvas state poisoned");
        if !state.frozen_spawn_specs.contains_key(&target) {
            return;
        }
        let rate_func = anim_type.default_rate_func();
        state.push_immediate(AnimationBuilder {
            target,
            anim_type,
            duration: 0.0,
            delay: 0.0,
            rate_func,
        });
    }

    /// Start hidden, wherever the drawable is created, until an animation
    /// in `SceneModel::play` shows it: `create`, `write`, `fade_in`, a move…
    /// Without it, a drawable created after the segment's first `play` is
    /// part of the segment's starting state, visible from its start. It must
    /// come before the drawable appears in a `play` or a cut; hiding a
    /// drawable already shown takes `fade_out` or `opacity(0)`.
    pub fn hidden(&self) -> Result<Self, &'static str> {
        let state = self.state.lock().expect("canvas state poisoned");
        if state.frozen_spawn_specs.contains_key(&self.id) {
            return Err(
                "hidden() must follow the drawable's creation; to hide one already shown,                  use fade_out() or opacity(0)",
            );
        }
        drop(state);
        self.defer_visibility_until_play();
        self.spec
            .lock()
            .expect("object spec poisoned")
            .hidden_by_author = true;
        Ok(self.clone())
    }

    /// Keep a generated reactive visual hidden until it is included in a
    /// `SceneModel::play` animation.
    pub(crate) fn defer_visibility_until_play(&self) {
        self.spec
            .lock()
            .expect("object spec poisoned")
            .defer_visibility_until_play = true;
    }

    pub(crate) fn with_style_targets(mut self, targets: Vec<SharedObjectSpec>) -> Self {
        self.style_targets = Arc::new(targets);
        self
    }

    pub(crate) fn inherited_style_targets(&self) -> Vec<SharedObjectSpec> {
        std::iter::once(self.spec.clone())
            .chain(self.style_targets.iter().cloned())
            .collect()
    }

    /// Apply a semantic part fill before structured text is compiled. Typst
    /// then carries the style through math shaping, including identifiers such
    /// as `theta` whose rendered glyphs no longer match the authored source.
    #[doc(hidden)]
    pub fn fill_text_part(&self, path: &[String], color: Color) -> bool {
        fn fill_part(
            content: &mut [gaanim_text::prelude::TextContent],
            path: &[String],
            color: Color,
        ) -> bool {
            let Some((name, remainder)) = path.split_first() else {
                return false;
            };
            for item in content {
                let gaanim_text::prelude::TextContent::Part(part) = item else {
                    continue;
                };
                if &part.name != name {
                    continue;
                }
                if remainder.is_empty() {
                    part.style.color = Some(color);
                    return true;
                }
                return fill_part(&mut part.content, remainder, color);
            }
            false
        }

        if path.is_empty() {
            return false;
        }
        // Once the text has been shown its declaration is frozen: the caller
        // recolors the part's glyphs at the cursor, as for any selection.
        if self
            .state
            .lock()
            .expect("canvas state poisoned")
            .frozen_spawn_specs
            .contains_key(&self.id)
        {
            return false;
        }
        let mut spec = self.spec.lock().expect("object spec poisoned");
        let SpawnKind::Text(text) = &mut spec.kind else {
            return false;
        };
        let changed = fill_part(&mut text.content, path, color);
        if changed {
            text.version = text.version.saturating_add(1);
        }
        changed
    }

    pub(crate) fn with_svg_parts(mut self, parts: HashMap<String, DrawableHandle>) -> Self {
        self.spec.lock().expect("object spec poisoned").svg_root = true;
        let owned = self
            .style_targets
            .iter()
            .chain(parts.values().map(|part| &part.spec));
        for spec in owned {
            let mut spec = spec.lock().expect("object spec poisoned");
            if !spec.svg_root {
                spec.svg_owner = Some(self.id);
            }
        }
        self.named_parts = Some(Arc::new(parts));
        self
    }

    /// The root of an imported glTF model: its named nodes, as parts, and
    /// its animations.
    pub(crate) fn with_gltf(
        mut self,
        parts: HashMap<String, DrawableHandle>,
        animations: Arc<gaanim_objects::gltf::GltfAnimations>,
    ) -> Self {
        self.named_parts = Some(Arc::new(parts));
        self.gltf = Some(animations);
        self
    }

    /// A glTF node, or a model's root, whose mesh's morph targets are
    /// `targets`.
    pub(crate) fn with_morph_targets(mut self, targets: Vec<Arc<GltfMorphTargets>>) -> Self {
        self.morph_targets = targets.into();
        self
    }

    /// Set the weight of the morph target `name` of a glTF node's mesh, or
    /// of every node of a model that has one by that name: 0 shows the mesh
    /// as modelled, 1 the target in full. Declared, it is the weight the
    /// mesh starts with; after the declaration, a cut at the cursor.
    pub fn morph(&self, name: &str, weight: f64) -> Result<Self, MorphTargetError> {
        let entries = morph_entries(&self.morph_targets, name, weight)?;
        for (_, spec, index) in &entries {
            let mut spec = spec.lock().expect("object spec poisoned");
            if let Some(weights) = &mut spec.morph_weights {
                let mut changed = weights.to_vec();
                if let Some(slot) = changed.get_mut(*index) {
                    *slot = weight as f32;
                }
                *weights = changed.into();
            }
        }
        self.push_immediate(
            self.id,
            AnimationType::Properties(PropertyAnimation {
                morph: entries
                    .iter()
                    .map(|(primitive, _, index)| (*primitive, *index, weight as f32))
                    .collect(),
                ..Default::default()
            }),
        );
        Ok(self.clone())
    }

    /// The names of the morph targets of a glTF node's mesh, or of all the
    /// nodes of a model, in the file's order and each once; empty for other
    /// drawables.
    pub fn morph_targets(&self) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for targets in self.morph_targets.iter() {
            for name in targets.names.iter() {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
        }
        names
    }

    /// The animations of an imported glTF model, in the file's order, with
    /// their durations in seconds; empty for other drawables.
    pub fn animations(&self) -> Vec<(String, f64)> {
        self.gltf
            .as_ref()
            .map(|animations| {
                animations
                    .clips
                    .iter()
                    .map(|clip| (clip.name.clone(), clip.duration))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Play the animation `name` of an imported glTF model: for `duration`
    /// seconds (by default once through, from `start_time`, at `speed`,
    /// unless `Scene.play` gives a duration),
    /// wrapping when `looped`, backwards when `reverse`, fading from the
    /// model's previous animation over `transition` seconds. Its time runs
    /// linearly unless the result is given an easing.
    #[allow(clippy::too_many_arguments)]
    pub fn animation(
        &self,
        name: &str,
        duration: Option<f64>,
        speed: f64,
        looped: bool,
        reverse: bool,
        transition: f64,
        start_time: f64,
    ) -> Result<Anim, GltfAnimationError> {
        let Some(animations) = &self.gltf else {
            return Err(GltfAnimationError::NotGltf);
        };
        if !speed.is_finite() || speed <= 0.0 {
            return Err(GltfAnimationError::InvalidSpeed);
        }
        let valid = |value: f64| value.is_finite() && value >= 0.0;
        if !valid(transition) || !valid(start_time) || duration.is_some_and(|value| !valid(value)) {
            return Err(GltfAnimationError::InvalidTiming);
        }
        let Some(clip) = animations.clip(name) else {
            return Err(GltfAnimationError::Unknown {
                name: name.to_owned(),
                available: animations
                    .clips
                    .iter()
                    .map(|clip| clip.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        };
        let length = animations.clips[clip].duration;
        // Once through: from the start time to its end, or a whole cycle
        // when it loops.
        let once = if looped {
            length
        } else {
            (length - start_time).max(0.0)
        };
        let anim = Anim::queued(
            self.id,
            AnimationType::GltfAnimation {
                animations: Arc::clone(animations),
                clip,
                speed,
                looped,
                reverse,
                transition,
                start_time,
            },
            self.state.clone(),
            self.segment_idx,
        );
        // A duration given to `Scene.play` replaces the natural one.
        Ok(match duration {
            Some(duration) => anim.duration(duration),
            None => anim.default_duration(once / speed),
        })
    }

    /// If this drawable is an axes, return its x/y ranges and full config.
    pub fn axes_info(&self) -> Option<(AxisRange, AxisRange, crate::canvas::AxesConfig)> {
        let spec = self.spec.lock().ok()?;
        if let crate::canvas::SpawnKind::Axes {
            x_range,
            y_range,
            config,
        } = &spec.kind
        {
            Some((*x_range, *y_range, config.clone()))
        } else {
            None
        }
    }

    /// An independent drawable declared like this one: its shape, style,
    /// effects and placement, with copies of its members and, for an
    /// imported SVG, of its named parts. Animations and updaters are not
    /// copied. The copy draws above the drawables declared before it.
    pub fn copy(&self) -> DrawableHandle {
        let spec = self.spec.lock().expect("object spec poisoned").clone();
        let mut copies = HashMap::new();
        let copy = super::duplicate::clone_spec_recording(&self.state, &spec, &mut copies);
        // Clips are operations, not declaration: clip the copies by the
        // copies of their masks (an SVG clip path is a member).
        {
            let mut state = self.state.lock().expect("canvas state poisoned");
            let clips: Vec<Op> = state
                .segments
                .iter()
                .flat_map(|segment| &segment.ops)
                .filter_map(|op| match op {
                    Op::SetClip {
                        target,
                        mask,
                        rule,
                        invert,
                    } => Some(Op::SetClip {
                        target: copies.get(target)?.id,
                        mask: mask.map(|mask| copies.get(&mask).map_or(mask, |copy| copy.id)),
                        rule: *rule,
                        invert: *invert,
                    }),
                    _ => None,
                })
                .collect();
            state.active_mut().ops.extend(clips);
        }
        let remap = |targets: &[Arc<GltfMorphTargets>]| -> Vec<Arc<GltfMorphTargets>> {
            targets
                .iter()
                .filter_map(|targets| targets.remapped(&copies))
                .collect()
        };
        let copy = copy.with_morph_targets(remap(&self.morph_targets));
        let Some(parts) = &self.named_parts else {
            return copy;
        };
        // Members of the copy still name the original SVG as their owner.
        for member in copies.values() {
            let mut member = member.spec.lock().expect("object spec poisoned");
            if member.svg_owner == Some(self.id) {
                member.svg_owner = Some(copy.id);
            }
        }
        let parts = parts
            .iter()
            .filter_map(|(name, part)| {
                let part = if part.id == self.id {
                    copy.clone()
                } else {
                    copies
                        .get(&part.id)?
                        .clone()
                        .with_morph_targets(remap(&part.morph_targets))
                };
                Some((name.clone(), part))
            })
            .collect();
        match &self.gltf {
            Some(animations) => copy.with_gltf(parts, Arc::clone(animations)),
            None => copy.with_svg_parts(parts),
        }
    }

    /// Resolve a named source group or path from an imported SVG.
    pub fn part(&self, id: &str) -> Result<DrawableHandle, SvgPartError> {
        let Some(parts) = &self.named_parts else {
            return Err(SvgPartError::NotSvg);
        };
        parts.get(id).cloned().ok_or_else(|| {
            let mut available = parts.keys().cloned().collect::<Vec<_>>();
            available.sort();
            SvgPartError::Unknown {
                id: id.to_owned(),
                available: available.join(", "),
            }
        })
    }

    /// Stable selectors available through [`DrawableHandle::part`].
    pub fn parts(&self) -> Vec<String> {
        let mut result = self
            .named_parts
            .as_ref()
            .map(|parts| parts.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        result.sort();
        result
    }

    fn push_layout(&self, op: LayoutOp) -> Self {
        use gaanim_animation::PropertyChannel;
        let channel = match &op {
            LayoutOp::SetTranslation(_)
            | LayoutOp::MoveAnchorTo { .. }
            | LayoutOp::MoveToAnchorPoint { .. }
            | LayoutOp::MoveTextAnchorTo { .. } => Some((PropertyChannel::Translation, false)),
            LayoutOp::ShiftBy(_) => Some((PropertyChannel::Translation, true)),
            LayoutOp::SetScale(_) | LayoutOp::SetScale3D(_) => {
                Some((PropertyChannel::Scale, false))
            }
            LayoutOp::ScaleBy(_) => Some((PropertyChannel::Scale, true)),
            LayoutOp::SetRotation(_) | LayoutOp::SetRotation3D(_) => {
                Some((PropertyChannel::Rotation, false))
            }
            LayoutOp::RotateBy(_) => Some((PropertyChannel::Rotation, true)),
            _ => None,
        };
        if let Some((channel, relative)) = channel {
            assert!(
                !relative || !self.property_is_bound(channel),
                "channel is reactively bound; animate its Parameter or assign a fixed value first"
            );
            if !relative {
                self.clear_property_binding(channel);
            }
        }
        let immediate = match &op {
            LayoutOp::MoveTextAnchorTo {
                target,
                anchor,
                center_multiline,
            } => {
                let horizontal = match anchor {
                    TextAnchor::BaselineLeft => -1.0,
                    TextAnchor::BaselineCenter => 0.0,
                    TextAnchor::BaselineRight => 1.0,
                };
                Some(AnimationType::PropertySource(
                    crate::anim::PropertySourceTarget::new(
                        gaanim_animation::PropertySources::TextTranslation {
                            values: [target.x.into(), target.y.into(), target.z.into()],
                            horizontal,
                            center_multiline: *center_multiline,
                        },
                    ),
                ))
            }
            // A group's pivot goes to the point, as when it is declared.
            LayoutOp::SetTranslation(to)
                if matches!(
                    self.spec.lock().expect("object spec poisoned").kind,
                    SpawnKind::Group(_)
                ) =>
            {
                Some(AnimationType::Properties(crate::anim::PropertyAnimation {
                    translation: Some(crate::anim::PropertyTranslation::To(*to)),
                    ..Default::default()
                }))
            }
            LayoutOp::SetTranslation(to) => Some(AnimationType::TranslateTo { to: *to }),
            LayoutOp::ShiftBy(delta) => Some(AnimationType::TranslateBy { delta: *delta }),
            LayoutOp::SetScale(factor) => Some(AnimationType::ScaleTo {
                to: DVec3::splat(*factor),
            }),
            LayoutOp::SetScale3D(to) => Some(AnimationType::ScaleTo { to: *to }),
            LayoutOp::ScaleBy(factor) if factor.x == factor.y && factor.y == factor.z => {
                Some(AnimationType::ScaleUniform { factor: factor.x })
            }
            LayoutOp::ScaleBy(factor) => Some(AnimationType::ScaleBy3D { factor: *factor }),
            LayoutOp::SetRotation(radians) => Some(AnimationType::RotateTo {
                to: DQuat::from_rotation_z(*radians),
            }),
            LayoutOp::SetRotation3D(euler) => Some(AnimationType::RotateTo {
                to: DQuat::from_euler(EulerRot::XYZ, euler.x, euler.y, euler.z),
            }),
            LayoutOp::RotateBy(delta) => Some(AnimationType::RotateBy3D { delta: *delta }),
            LayoutOp::SetSkew(to) => Some(AnimationType::SkewTo { to: *to }),
            LayoutOp::MoveAnchorTo { target, anchor } => Some(AnimationType::TranslateAnchorTo {
                to: *target,
                anchor: *anchor,
            }),
            LayoutOp::MoveToAnchorPoint { point } => {
                Some(AnimationType::TranslateToAnchorPoint { point: *point })
            }
            LayoutOp::SetPivot(pivot) => Some(AnimationType::PivotTo { pivot: *pivot }),
            _ => None,
        };
        let result = self.update_spec(|spec| {
            let positional = matches!(
                op,
                LayoutOp::SetTranslation(_)
                    | LayoutOp::ShiftBy(_)
                    | LayoutOp::MoveAnchorTo { .. }
                    | LayoutOp::MoveToAnchorPoint { .. }
                    | LayoutOp::MoveTextAnchorTo { .. }
                    | LayoutOp::NextTo { .. }
                    | LayoutOp::AlignTo { .. }
                    | LayoutOp::ToEdge { .. }
                    | LayoutOp::ToCorner { .. }
            );
            assert!(
                !(positional && spec.layout_owner.is_some()),
                "layout owns this drawable's position; use .item(offset=...) or absolute placement"
            );
            spec.layout_ops.push(op);
        });
        if let Some(anim_type) = immediate {
            self.push_immediate(self.id, anim_type);
        }
        result
    }

    pub fn layout_owner(&self) -> Option<ObjectId> {
        self.spec.lock().expect("object spec poisoned").layout_owner
    }

    /// How this drawable sits in the box that contains it.
    pub fn layout_item(&self) -> gaanim_layout::LayoutItemStyle {
        self.spec
            .lock()
            .expect("object spec poisoned")
            .layout_item
            .clone()
            .unwrap_or_default()
    }

    /// The item properties set on this drawable, if any.
    pub fn explicit_layout_item(&self) -> Option<gaanim_layout::LayoutItemStyle> {
        self.spec
            .lock()
            .expect("object spec poisoned")
            .layout_item
            .clone()
    }

    /// Set how this drawable sits in the box that contains it.
    pub fn set_layout_item(&self, item: gaanim_layout::LayoutItemStyle) {
        self.spec.lock().expect("object spec poisoned").layout_item = Some(item);
    }

    // -- Instant setters (return Self) --

    pub fn fill(self, color: Color) -> Self {
        self.fill_brush(Brush::Solid(color))
    }

    pub fn material(
        self,
        material: gaanim_scene::Material3D,
    ) -> Result<Self, Primitive3DHandleError> {
        {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let SpawnKind::Primitive3D(mesh) = &mut spec.kind else {
                return Err(Primitive3DHandleError);
            };
            mesh.material = Some(material);
            spec.material_animation_cursor = Some(material);
            if let Some(extrusion) = &mut spec.extrusion {
                extrusion.material = Some(material);
            }
        }
        Ok(self)
    }

    pub fn material_to(
        &self,
        material: gaanim_scene::Material3D,
    ) -> Result<Anim, Primitive3DHandleError> {
        let from = {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            let SpawnKind::Primitive3D(mesh) = &spec.kind else {
                return Err(Primitive3DHandleError);
            };
            let from = spec
                .material_animation_cursor
                .or(mesh.material)
                .unwrap_or_default();
            spec.material_animation_cursor = Some(material);
            from
        };
        Ok(self.anim(AnimationType::Material3DTo { from, to: material }))
    }

    pub fn fill_brush(self, brush: Brush) -> Self {
        self.update_style(|spec| {
            spec.fill = Some(brush.clone());
            spec.fill_overridden = true;
        })
    }

    pub fn no_fill(self) -> Self {
        self.update_style(|spec| {
            spec.fill = None;
            spec.fill_overridden = true;
        })
    }

    pub fn stroke(self, color: Color, width: f64) -> Self {
        self.stroke_brush(Brush::Solid(color), width)
    }

    /// Stroke with `brush` at `width`, keeping the caps, joins and dashes
    /// of a stroke style set before.
    pub fn stroke_brush(self, brush: Brush, width: f64) -> Self {
        self.update_style(|spec| {
            spec.stroke = Some((brush.clone(), width));
            let mut style = spec
                .stroke_style
                .take()
                .unwrap_or_else(|| gaanim_core::kurbo::Stroke::new(width));
            style.width = width;
            spec.stroke_style = Some(style);
            spec.stroke_overridden = true;
        })
    }

    /// Place the stroke inside, centered on, or outside closed contours,
    /// including every text glyph. Open paths always center their stroke.
    /// This is declaration state: it is not animated or cut on the timeline.
    pub fn stroke_align(self, align: gaanim_renderer::effects::StrokeAlign) -> Self {
        self.update_style(|spec| spec.stroke_align = Some(align))
    }

    /// Let the stroke pen follow the scale the drawable accumulates from
    /// itself, its groups and coordinate views (`enabled`), or keep it in
    /// scene units (the default). Applies to every stroke of a group or an
    /// imported SVG. This is declaration state: it is not animated or cut.
    pub fn scale_stroke_with_object(self, enabled: bool) -> Self {
        self.update_style(|spec| spec.stroke_scales_with_object = Some(enabled))
    }

    /// Draw this drawable and its descendants through post-process
    /// `passes`, as one layer in a texture `margin` scene units wider than
    /// them; no passes removes the effect. Declaration state: it applies
    /// for the whole timeline.
    pub fn shader_effect(
        self,
        passes: Vec<gaanim_renderer::post_process::PostProcessPass>,
        margin: f64,
    ) -> Self {
        let effect = (!passes.is_empty()).then(|| super::types::ShaderEffectSpec {
            passes,
            margin: if margin.is_finite() {
                margin.max(0.0)
            } else {
                0.0
            },
        });
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_shader_effect(&self.spec, effect);
        self
    }

    /// Trace an `outline` around what this drawable and its descendants
    /// cover, from their alpha, like a sticker's border: drawn beneath them,
    /// after any shader effect's passes, and with its shadow cast from the
    /// result. `None` removes it. Declaration state: it applies for the
    /// whole timeline.
    pub fn outline(self, outline: Option<super::types::OutlineSpec>) -> Self {
        let outline = outline.filter(|outline| outline.width.is_finite() && outline.width > 0.0);
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_outline(&self.spec, outline);
        self
    }

    /// The scene this drawable belongs to, while it is shared.
    pub fn owning_scene(&self) -> Option<std::sync::Arc<std::sync::Mutex<super::SceneModel>>> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .owner
            .as_ref()?
            .upgrade()
    }

    /// Show this drawable and its descendants only through `source`, a
    /// track matte: where `source` is opaque (`Alpha`) or bright (`Luma`),
    /// or the opposite with the inverted modes. `source` stops being drawn
    /// on its own; it still moves and animates. `None` removes the matte.
    /// Declaration state: it applies for the whole timeline.
    pub fn matte(
        self,
        source: Option<&DrawableHandle>,
        mode: gaanim_renderer::object_effects::MatteMode,
    ) -> Result<Self, String> {
        if let Some(source) = source {
            if !self.same_canvas(source) {
                return Err("the matte belongs to another scene".to_string());
            }
            if source.id == self.id {
                return Err("a drawable cannot be its own matte".to_string());
            }
        }
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_matte(&self.spec, source.map(|source| (source.id, mode)));
        Ok(self)
    }

    /// Make this drawable frosted glass (see
    /// [`gaanim_renderer::object_effects::Glass`]): what is drawn behind it
    /// shows through its outline, blurred and bent; the drawable itself is
    /// drawn above, so a translucent fill tints it. `None` removes it.
    /// Declaration state: it applies for the whole timeline.
    pub fn glass(self, glass: Option<gaanim_renderer::object_effects::Glass>) -> Self {
        let glass = glass.map(|glass| {
            let finite = |value: f64, fallback: f64| {
                if value.is_finite() {
                    value.max(0.0)
                } else {
                    fallback
                }
            };
            gaanim_renderer::object_effects::Glass {
                blur: finite(glass.blur, 0.0),
                saturation: finite(glass.saturation, 1.0),
                refraction: finite(glass.refraction, 0.0),
                edge: finite(glass.edge, 0.0).min(1.0),
                dispersion: finite(glass.dispersion, 0.0).min(1.0),
                bevel: finite(glass.bevel, 0.0),
                transparency: finite(glass.transparency, 1.0).min(1.0),
                twist: if glass.twist.is_finite() {
                    glass.twist.clamp(-2.0, 2.0)
                } else {
                    0.0
                },
            }
        });
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_glass(&self.spec, glass);
        self
    }

    /// Draw the fill and stroke as chalk: the outline trembles by up to
    /// `roughness` scene units and a grain breaks the paint, both following
    /// `seed`. Applies to every glyph of a text and every child of a group,
    /// for the whole scene: chalk is not animated or cut on the timeline.
    pub fn chalk(self, seed: u64, roughness: f64) -> Self {
        let chalk = gaanim_renderer::effects::ChalkBrush { seed, roughness };
        let mut state = self.state.lock().expect("canvas state poisoned");
        for spec in std::iter::once(&self.spec).chain(self.style_targets.iter()) {
            state.set_chalk(spec, chalk);
        }
        drop(state);
        self
    }

    /// Apply a complete native stroke, including cap, join, miter and dashes.
    pub fn stroke_with_style(self, brush: Brush, style: gaanim_core::kurbo::Stroke) -> Self {
        self.update_style(|spec| {
            spec.stroke = Some((brush.clone(), style.width));
            spec.stroke_style = Some(style.clone());
            spec.stroke_overridden = true;
        })
    }

    pub fn no_stroke(self) -> Self {
        self.update_style(|spec| {
            spec.stroke = None;
            spec.stroke_style = None;
            spec.stroke_overridden = true;
        })
    }

    /// Attach an ordered theme class to this drawable and its visual style targets.
    pub fn style_class(self, name: impl Into<String>) -> Result<Self, String> {
        let name = name.into();
        if name.is_empty()
            || name
                .chars()
                .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')))
        {
            return Err("style class names must use letters, digits, '_' or '-'".into());
        }
        Ok(self.update_style(|spec| spec.style_classes.push(name.clone())))
    }

    /// Add a soft outer glow. The effect is compiled into the retained vector fragment.
    pub fn glow(self, color: Color, radius: f64, intensity: f32) -> Self {
        self.update_style(|spec| {
            spec.glow = Some(gaanim_renderer::effects::Glow {
                color,
                radius,
                intensity,
            });
        })
    }

    /// Declaration-time scene position (folded from `move_to`/`shift_by`),
    /// or `None` when it depends on layout resolved at compile time.
    pub fn authored_position(&self) -> Option<DVec2> {
        super::canvas_impl::authored_position(&self.spec.lock().expect("object spec poisoned"))
    }

    /// Show only part of the drawn path: the window `[start, end]` shifted
    /// by `offset` (arc-length fractions). `None` keeps the current value;
    /// `sequential` measures the window across all sub-paths.
    pub fn trim(
        self,
        start: Option<f64>,
        end: Option<f64>,
        offset: Option<f64>,
        sequential: Option<bool>,
    ) -> Self {
        let rate_func = gaanim_math::RateFunc::Linear;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .push_immediate(AnimationBuilder {
                target: self.id,
                anim_type: AnimationType::PathTrim {
                    start,
                    end,
                    offset,
                    sequential,
                },
                duration: 0.0,
                delay: 0.0,
                rate_func,
            });
        self
    }

    /// Apply a soft vector blur to this drawable; a `sigma` of zero (or
    /// less) removes the blur, as `animate().blur(None)` does.
    pub fn blur(self, sigma: f64) -> Self {
        self.update_style(|spec| {
            spec.blur = (sigma > 0.0).then_some(gaanim_renderer::effects::GaussianBlur { sigma });
        })
    }

    /// Add a soft shadow behind this drawable.
    pub fn shadow(self, color: Color, offset: DVec2, blur_radius: f64) -> Self {
        self.update_style(|spec| {
            spec.shadow = Some(gaanim_renderer::effects::DropShadow {
                color,
                offset,
                blur_radius,
            });
        })
    }

    /// Composite this drawable with what is drawn beneath it using `mode`
    /// (e.g. `Mix::Screen` for lights, `Mix::Multiply` for markers).
    ///
    /// Like `fill`, the mode of a group or text reaches every member; a
    /// member restyled afterwards keeps its own, including the plain
    /// `BlendMode::default()`. `None` clears the mode.
    pub fn blend(self, mode: Option<gaanim_core::peniko::BlendMode>) -> Self {
        self.update_style(|spec| spec.blend = mode)
    }

    /// Trail this drawable with fading copies of itself as it was earlier
    /// (see [`EchoSpec`]); `None` removes them.
    ///
    /// Each copy replays the drawable's own animations with a delay, so it
    /// is exact at any seek, in every export format and in SVG. Copies share
    /// the drawable's parent and draw beneath it; they are hidden while the
    /// drawable is hidden and do not reach back across a segment cut. Motion
    /// from updaters, reactive positions and parent groups is not delayed.
    pub fn echo(self, echo: Option<super::types::EchoSpec>) -> Self {
        self.update_style(|spec| spec.echo = echo)
    }

    /// Stretch the drawable along its velocity and squash it across, keeping
    /// its area: the stretch is `1 + amount * speed` (scene units per
    /// second), at most `max_ratio`. The velocity comes from the drawable's
    /// own animations, so a drawable at rest is never deformed; members of
    /// a group deform with it. `amount = 0` removes the effect.
    pub fn squash_stretch(self, amount: f64, max_ratio: f64) -> Result<Self, String> {
        if !(amount.is_finite() && amount >= 0.0) {
            return Err(format!("amount must be a finite number >= 0, got {amount}"));
        }
        if !(max_ratio.is_finite() && max_ratio >= 1.0) {
            return Err(format!(
                "max_ratio must be a finite number >= 1, got {max_ratio}"
            ));
        }
        let squash = (amount > 0.0).then_some((amount, max_ratio));
        Ok(self.update_style(|spec| spec.squash_stretch = squash))
    }

    /// Shape the stroke width along the visible path with `(position,
    /// factor)` pairs: positions in `[0, 1]` of the arc length, factors that
    /// scale the stroke width, interpolated linearly. The stroke is drawn as
    /// a filled outline, so dashes do not apply; trims and `create` shape the
    /// visible part. `None` restores the plain pen. Applies to every member
    /// of a group or text.
    pub fn stroke_profile(self, profile: Option<Vec<(f64, f64)>>) -> Result<Self, String> {
        let profile = match profile {
            None => None,
            Some(mut points) => {
                if points.is_empty() {
                    return Err("a stroke profile needs at least one point".to_string());
                }
                if points.iter().any(|&(position, factor)| {
                    !(0.0..=1.0).contains(&position) || !factor.is_finite() || factor < 0.0
                }) {
                    return Err(
                        "profile positions must be in [0, 1] and factors finite and >= 0"
                            .to_string(),
                    );
                }
                points.sort_by(|a, b| a.0.total_cmp(&b.0));
                Some(std::sync::Arc::<[(f64, f64)]>::from(points))
            }
        };
        Ok(self.update_style(|spec| spec.stroke_profile = profile.clone()))
    }

    /// Taper the stroke to a point over the first `start` and last `end`
    /// fractions of the visible path, like a brush stroke.
    pub fn stroke_taper(self, start: f64, end: f64) -> Result<Self, String> {
        if !(0.0..=1.0).contains(&start) || !(0.0..=1.0).contains(&end) || start + end > 1.0 {
            return Err(format!(
                "start and end must be in [0, 1] with start + end <= 1, got {start} and {end}"
            ));
        }
        let mut points = vec![(0.0, if start > 0.0 { 0.0 } else { 1.0 })];
        if start > 0.0 {
            points.push((start, 1.0));
        }
        if end > 0.0 {
            points.push((1.0 - end, 1.0));
        }
        points.push((1.0, if end > 0.0 { 0.0 } else { 1.0 }));
        self.stroke_profile(Some(points))
    }

    /// Whether the scene's motion blur smears this drawable (the default).
    /// A drawable with `false`, and its members, draws in every sub-frame as
    /// it is at the frame time, e.g. to keep a title or HUD sharp while the
    /// camera moves.
    pub fn motion_blur(self, enabled: bool) -> Self {
        self.update_style(|spec| spec.motion_blur_exempt = !enabled)
    }

    /// Whether the scene's post-process (the canvas's, or a segment's)
    /// applies to this drawable (the default). A drawable with `false`, and
    /// its members, draws above the processed frame, e.g. to keep a label
    /// readable under pixelation.
    pub fn post_process(self, enabled: bool) -> Self {
        self.update_style(|spec| spec.post_exempt = !enabled)
    }

    /// Step this drawable's time like stop-motion: its animations, updaters,
    /// procedural motion (wiggle, oscillate, spin), path modifiers, dash
    /// flow and video or Lottie frames, and those of its members, are
    /// evaluated `rate` times a second and hold in between. The steps sit
    /// on a grid from timeline second 0, so drawables with the same rate
    /// change together, and an animation still ends on its final value at
    /// its end. `None` restores smooth time. A rate that is not positive
    /// and finite is an error.
    pub fn step_rate(self, rate: Option<f64>) -> Result<Self, String> {
        if rate.is_some_and(|rate| !(rate.is_finite() && rate > 0.0)) {
            return Err("step_rate must be a positive number of steps per second".into());
        }
        Ok(self.update_style(|spec| spec.step_rate = rate))
    }

    /// Draw `start` and `end` tips on the path's ends, filled with its stroke
    /// paint; both `None` removes them. Arrowheads shorten the stroke under
    /// them and follow the path's current ends, so trims, `create` and
    /// regenerated paths carry them. `length` and `width` default to sizes
    /// that scale with the stroke width.
    pub fn tip(
        self,
        start: Option<gaanim_animation::TipKind>,
        end: Option<gaanim_animation::TipKind>,
        length: Option<f64>,
        width: Option<f64>,
    ) -> Result<Self, String> {
        if [length, width]
            .into_iter()
            .flatten()
            .any(|size| !size.is_finite() || size <= 0.0)
        {
            return Err("tip length and width must be finite and positive".into());
        }
        let tips = (start.is_some() || end.is_some())
            .then(|| gaanim_animation::StrokeTips::new(start, end, length, width));
        Ok(self.update_style(|spec| spec.tips = tips.clone()))
    }

    /// Remove all visual effects while preserving fill and stroke styling.
    pub fn no_effects(self) -> Self {
        self.update_style(|spec| {
            spec.glow = None;
            spec.blur = None;
            spec.shadow = None;
        })
    }

    /// Clip this drawable to another drawable's vector geometry.
    ///
    /// The mask keeps its own visibility, so call `mask.no_fill().no_stroke()`
    /// when it should act only as clipping geometry.
    pub fn clip(self, mask: &DrawableHandle, rule: gaanim_core::peniko::Fill) -> Self {
        self.clip_with(
            mask,
            ClipOptions {
                rule,
                invert: false,
            },
        )
    }

    /// Clip this drawable with explicit fill-rule and inversion options.
    pub fn clip_with(self, mask: &DrawableHandle, options: ClipOptions) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: Some(mask.id),
                rule: options.rule,
                invert: options.invert,
            });
        self
    }

    /// Remove a previously configured clipping mask.
    pub fn no_clip(self) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::SetClip {
                target: self.id,
                mask: None,
                rule: gaanim_core::peniko::Fill::NonZero,
                invert: false,
            });
        self
    }

    /// Reveal this drawable behind a straight edge that sweeps across it
    /// in `direction` as `reveal` goes from 0 (hidden) to 1 (whole). The
    /// edge is placed from the drawable's live bounds every frame, so the
    /// wipe follows later moves and resizes. It clips the drawable,
    /// replacing any clip it had. Returns the hidden mask.
    pub fn wipe(
        &self,
        reveal: ScalarSource,
        direction: Direction,
    ) -> Result<DrawableHandle, InvalidWipe> {
        wipe_mask(&self.state, self.id, reveal, direction)
    }

    /// Colors every matching text or equation fragment in this drawable.
    ///
    /// Matching is case-insensitive and tolerant of math formatting, so a
    /// query such as `"mc"` can target `m c` in a Typst equation. Typst symbol
    /// names such as `theta` and `sum` resolve to their rendered Unicode when
    /// no literal match exists. Later calls take precedence when selections
    /// overlap.
    pub fn color_by(self, fragment: impl Into<String>, color: Color) -> Self {
        self.color_fragment(fragment.into(), None, color)
    }

    /// Colors only the `occurrence`-th match of `fragment`, counted as
    /// [`Self::select_nth`] counts them.
    pub fn color_nth(self, fragment: impl Into<String>, occurrence: usize, color: Color) -> Self {
        self.color_fragment(fragment.into(), Some(occurrence), color)
    }

    fn color_fragment(self, fragment: String, occurrence: Option<usize>, color: Color) -> Self {
        self.update_spec(|spec| {
            if !fragment.is_empty() {
                spec.fragment_fills.push((fragment, occurrence, color));
            }
        })
    }

    /// Creates a deferred selection for glyph-level styling and emphasis.
    /// Typst symbol names are resolved through Codex when literal matching
    /// finds nothing.
    pub fn select(&self, fragment: impl Into<String>) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: None,
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        }
    }

    /// Creates a deferred selection for one zero-based occurrence of a fragment.
    /// Literal occurrences take precedence over Typst/Codex symbol matching.
    pub fn select_nth(&self, fragment: impl Into<String>, occurrence: usize) -> FragmentSelection {
        FragmentSelection {
            target: self.id,
            fragment: fragment.into(),
            occurrence: Some(occurrence),
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        }
    }

    /// Registers a semantic name for a text or equation fragment.
    pub fn define_tag(
        self,
        name: impl Into<String>,
        fragment: impl Into<String>,
        occurrence: Option<usize>,
    ) -> Self {
        let name = name.into();
        let fragment = fragment.into();
        self.update_spec(|spec| {
            if !name.trim().is_empty() && !fragment.trim().is_empty() {
                spec.fragment_tags.push((name, fragment, occurrence));
            }
        })
    }

    /// Selects a previously registered semantic tag.
    pub fn tag(&self, name: &str) -> Option<FragmentSelection> {
        let (fragment, occurrence) = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .fragment_tags
            .iter()
            .rev()
            .find(|(tag, _, _)| tag == name)
            .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))?;
        Some(FragmentSelection {
            target: self.id,
            fragment,
            occurrence,
            state: self.state.clone(),
            layout_owner: self.layout_owner(),
        })
    }

    /// Writes semantic terms in tag order instead of staggering individual
    /// glyphs. If `tags` is omitted, all declared tags are used in declaration
    /// order.
    pub fn write_by_parts(&self, paths: Option<Vec<String>>, duration: f64) -> Self {
        if !duration.is_finite() || duration <= 0.0 {
            return self.clone();
        }
        let spec = self.spec.lock().expect("object spec poisoned").clone();
        let declared = spec.fragment_tags;
        let terms: Vec<(String, Option<usize>)> = match paths {
            Some(names) => names
                .into_iter()
                .filter_map(|name| {
                    declared
                        .iter()
                        .rev()
                        .find(|(tag, _, _)| tag == &name)
                        .map(|(_, fragment, occurrence)| (fragment.clone(), *occurrence))
                })
                .collect(),
            None if !declared.is_empty() => declared
                .into_iter()
                .map(|(_, fragment, occurrence)| (fragment, occurrence))
                .collect(),
            None => match spec.kind {
                SpawnKind::Text(spec) => math_source_terms(&spec.plain_text())
                    .into_iter()
                    .map(|fragment| (fragment, None))
                    .collect(),
                _ => Vec::new(),
            },
        };
        if !terms.is_empty() {
            self.state.lock().expect("canvas state poisoned").segments[self.segment_idx]
                .ops
                .push(Op::WriteTerms {
                    target: self.id,
                    terms,
                    duration,
                });
        }
        self.clone()
    }

    /// Sets the initial value of a `ValueTracker` before the scene is compiled.
    /// Calling this on a regular drawable has no effect.
    pub fn set_value(self, value: f64) -> Self {
        let result = self.update_spec(|spec| {
            if let SpawnKind::ValueTracker(current) = &mut spec.kind {
                *current = value;
            }
        });
        self.push_immediate(self.id, AnimationType::SignalFloat { to: value });
        result
    }

    pub fn opacity(self, op: impl Into<ScalarSource>) -> Self {
        let op = op.into();
        let Some(op) = op.constant_value() else {
            return self
                .bind_property(PropertySources::Opacity(op))
                .expect("invalid reactive opacity source");
        };
        let op = op as f32;
        self.clear_property_binding(gaanim_animation::PropertyChannel::Opacity);
        // Opacity multiplies down the hierarchy, so a group's opacity is its
        // own value, like `animate().opacity(...)`. Writing it into member
        // leaves too would square it, and a later animation of the group
        // could never undo a zero.
        self.restyle(std::iter::once(&self.spec), |spec| {
            spec.opacity = op;
            spec.opacity_overridden = true;
        })
    }

    /// Shows the first `count` copies of a `repeat` or `duplicate` group; a
    /// fractional count fades the next copy. Declared before the first
    /// `play`, it is the initial count; later it cuts at the cursor.
    pub fn count(self, count: f64) -> Result<Self, String> {
        let (copies, from) = self
            .spec
            .lock()
            .expect("object spec poisoned")
            .repeat_count
            .ok_or("count() requires a group made by repeat() or duplicate()")?;
        super::duplicate::check_count(count, copies)?;
        self.update_spec(|spec| {
            if let Some((_, cursor)) = &mut spec.repeat_count {
                *cursor = count;
            }
        });
        if from != count {
            let properties = PropertyAnimation {
                count: Some((from, count)),
                ..PropertyAnimation::default()
            };
            self.push_immediate(self.id, AnimationType::Properties(properties));
        }
        Ok(self)
    }

    /// Emits `count` particles at once from this particle emitter at the
    /// cursor, without moving it. Before the first `play` the burst happens
    /// at the declaration time.
    pub fn burst(self, count: u32) -> Result<Self, String> {
        if super::particles::emitter_longest_life(
            &self.spec.lock().expect("object spec poisoned").kind,
        )
        .is_none()
        {
            return Err("burst() requires a particle emitter made by scene.fx".to_string());
        }
        super::particles::check_burst(count)?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::ParticleBurst {
                target: self.id,
                count,
            });
        Ok(self)
    }

    /// Set every vertex of a polygon or polyline, in the coordinates it was
    /// declared in. Before the first play this is the declared shape; after
    /// it, a cut at the cursor that `animate.points` can continue from.
    pub fn points(self, points: Vec<(f64, f64)>) -> Result<Self, String> {
        let declared = declared_vertices(&self.spec.lock().expect("object spec poisoned").kind)?;
        if points.len() != declared {
            return Err(format!(
                "points() needs {declared} points, one per vertex of the shape, got {}",
                points.len()
            ));
        }
        if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Err("points must be finite".to_string());
        }
        let this = self.update_spec(|spec| {
            if let SpawnKind::Polygon(vertices)
            | SpawnKind::Polyline {
                points: vertices, ..
            } = &mut spec.kind
            {
                *vertices = points.clone();
            }
        });
        this.push_immediate(this.id, AnimationType::PathPointsTo { points });
        Ok(this)
    }

    pub fn z_index(self, z: i32) -> Self {
        self.update_spec(|spec| spec.z_index = z)
    }

    /// Draw this drawable, and its members, in the layer `name` named with
    /// [`SceneModel::z_layers`](super::SceneModel::z_layers), above every
    /// layer behind it whatever their `z_index`; `z_index` orders it within
    /// its layer. Draw order holds for the whole scene, so a call after the
    /// drawable is shown applies from its start.
    pub fn z_layer(self, name: &str) -> Result<Self, String> {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_z_layer(&self.spec, name)?;
        Ok(self)
    }

    /// Let this drawable, and its members, hide texts on purpose: the
    /// covered-text warning of `gaanim check` skips the texts it covers.
    /// Nothing changes in the drawing; it holds for the whole scene.
    pub fn covers_on_purpose(self) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .set_covers_on_purpose(&self.spec);
        self
    }

    pub fn move_to(self, x: impl Into<ScalarSource>, y: impl Into<ScalarSource>) -> Self {
        let x = x.into();
        let y = y.into();
        let (Some(x), Some(y)) = (x.constant_value(), y.constant_value()) else {
            return self
                .bind_property(PropertySources::Translation {
                    values: [x, y, 0.0.into()],
                    anchor: None,
                })
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetTranslation(DVec3::new(x, y, 0.0)))
    }

    pub fn shift_by(self, dx: f64, dy: f64) -> Self {
        self.push_layout(LayoutOp::ShiftBy(DVec3::new(dx, dy, 0.0)))
    }

    pub fn shift_by_3d(self, dx: f64, dy: f64, dz: f64) -> Self {
        self.push_layout(LayoutOp::ShiftBy(DVec3::new(dx, dy, dz)))
    }

    /// Apply the default 2D placement semantics used by the Python API.
    ///
    /// Ordinary drawables place their visual center at the target. Identity
    /// groups preserve an authored local coordinate frame, so their local
    /// origin is placed at the target instead. Coordinate systems use identity
    /// groups specifically so labels cannot displace their mathematical origin.
    /// A group about an origin places that origin.
    pub fn move_to_default(self, x: f64, y: f64) -> Self {
        let preserves_authored_origin = matches!(
            self.spec.lock().expect("object spec poisoned").kind,
            SpawnKind::GroupNoCenter(_)
        ) || self.has_group_origin();
        if preserves_authored_origin {
            self.move_to(x, y)
        } else {
            self.at_anchor(x, y, Anchor::Center)
        }
    }

    /// Whether this is a group about an origin, which `move_to_default`
    /// places in place of its box's center.
    #[doc(hidden)]
    pub fn has_group_origin(&self) -> bool {
        let spec = self.spec.lock().expect("object spec poisoned");
        matches!(spec.kind, SpawnKind::Group(_)) && spec.group_origin.is_some()
    }

    /// 3D position in world space (perspective-aware).
    pub fn move_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .bind_property(PropertySources::Translation {
                    values: [x, y, z],
                    anchor: None,
                })
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetTranslation(DVec3::new(x, y, z)))
    }

    /// Makes this drawable always face the camera (billboard) in 3D.
    /// Chainable: `scene.text("label").move_to_3d(x,y,z).billboard()`
    pub fn billboard(self) -> Self {
        self.update_spec(|spec| spec.billboard = true)
    }

    /// A billboard whose box's `anchor` lies on its point, moved by `offset`
    /// scene units of the frame on screen, however far the point is. Its
    /// members, inside a group, are placed in those units too.
    pub fn billboard_shifted(self, offset: DVec2, anchor: Option<Anchor>) -> Self {
        self.update_spec(|spec| {
            spec.billboard = true;
            spec.billboard_shift = Some(gaanim_scene::BillboardShift {
                offset,
                anchor: anchor.map(|anchor| anchor.to_offset()),
                ..Default::default()
            });
        })
    }

    /// Places this drawable in the 3D world: `move_to_3d` and `rotate_to_3d`
    /// tilt it under perspective, meshes in front hide it pixel by pixel, and
    /// with a `material` it is lit as a surface. It keeps its animations.
    /// Chainable: `scene.text("sign").in_3d(None).rotate_to_3d(-1.2, 0, 0)`
    pub fn in_3d(self, material: Option<gaanim_scene::Material3D>) -> Self {
        self.update_spec(|spec| spec.in_3d = Some(gaanim_scene::InWorld3D { material }))
    }

    /// [`Self::in_3d`] on a plane of the model: the drawing's x axis along
    /// `x_axis`, its y axis along `y_axis` (squared to it), and its scene
    /// origin at `origin`, so each point `p` of the drawing as it stands
    /// lands at `origin + rotation * p`. The drawing's origin becomes its
    /// pivot, which it turns about from then on.
    pub fn in_3d_on_plane(
        self,
        material: Option<gaanim_scene::Material3D>,
        x_axis: DVec3,
        y_axis: DVec3,
        origin: DVec3,
    ) -> Result<Self, InvalidOrientation> {
        let rotation =
            gaanim_math::orientation_from_axes(x_axis, y_axis).ok_or(InvalidOrientation)?;
        if !origin.is_finite() {
            return Err(InvalidOrientation);
        }
        // The shift takes each point p to origin + p, and turning about
        // `origin` then takes it to origin + rotation * p, at declaration and
        // at the cursor alike.
        Ok(self
            .in_3d(material)
            .shift_by_3d(origin.x, origin.y, origin.z)
            .with_pivot_3d(origin.x, origin.y, origin.z)
            .rotate_by_rotation(rotation))
    }

    /// Makes this drawable a fixed HUD overlay (screen-space, not affected by 3D camera).
    /// Chainable: `scene.text("title").hud().move_to(0, 300)`
    pub fn hud(self) -> Self {
        self.update_spec(|spec| spec.hud = true)
    }

    pub fn scale_to(self, factor: impl Into<ScalarSource>) -> Self {
        let factor = factor.into();
        let Some(factor) = factor.constant_value() else {
            return self
                .bind_property(PropertySources::Scale([
                    factor.clone(),
                    factor.clone(),
                    factor,
                ]))
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetScale(factor))
    }

    pub fn scale_by(self, factor: f64) -> Self {
        self.push_layout(LayoutOp::ScaleBy(DVec3::splat(factor)))
    }

    /// Apply a general 2D linear map about the pivot, replacing the current
    /// rotation, skew and scale: `(x, y) -> (a x + b y, c x + d y)` for rows
    /// `[[a, b], [c, d]]`. It can draw a face in any oblique or isometric
    /// projection without cutting it up.
    pub fn matrix_to(self, map: super::LinearMap2D) -> Self {
        let parts = map.parts();
        self.rotate_to(parts.angle)
            .skew_to(parts.skew, 0.0)
            .scale_to_3d(parts.scale_x, parts.scale_y, 1.0)
    }

    pub fn scale_by_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::ScaleBy(DVec3::new(x, y, z)))
    }

    pub fn scale_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .bind_property(PropertySources::Scale([x, y, z]))
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetScale3D(DVec3::new(x, y, z)))
    }

    pub fn rotate_to(self, radians: impl Into<ScalarSource>) -> Self {
        let radians = radians.into();
        let Some(radians) = radians.constant_value() else {
            return self
                .bind_property(PropertySources::Rotation([0.0.into(), 0.0.into(), radians]))
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetRotation(radians))
    }

    pub fn rotate_by(self, radians: f64) -> Self {
        self.push_layout(LayoutOp::RotateBy(DQuat::from_rotation_z(radians)))
    }

    /// Shear about the pivot: `x` moves points horizontally by `x` times their
    /// height above it, `y` vertically by `y` times their offset to its right.
    pub fn skew_to(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetSkew(DVec2::new(x, y)))
    }

    pub fn rotate_by_3d(self, axis: &str, radians: f64) -> Result<Self, RotationAxisError> {
        let delta = match axis.to_ascii_lowercase().as_str() {
            "x" => DQuat::from_rotation_x(radians),
            "y" => DQuat::from_rotation_y(radians),
            "z" => DQuat::from_rotation_z(radians),
            _ => {
                return Err(RotationAxisError {
                    axis: axis.to_owned(),
                });
            }
        };
        Ok(self.push_layout(LayoutOp::RotateBy(delta)))
    }

    /// Turn the drawable by `rotation` about its pivot, after the rotation
    /// it has.
    pub fn rotate_by_rotation(self, rotation: DQuat) -> Self {
        self.push_layout(LayoutOp::RotateBy(rotation))
    }

    /// Set XYZ Euler rotation in radians.
    pub fn rotate_to_3d(
        self,
        x: impl Into<ScalarSource>,
        y: impl Into<ScalarSource>,
        z: impl Into<ScalarSource>,
    ) -> Self {
        let x = x.into();
        let y = y.into();
        let z = z.into();
        let (Some(x), Some(y), Some(z)) =
            (x.constant_value(), y.constant_value(), z.constant_value())
        else {
            return self
                .bind_property(PropertySources::Rotation([x, y, z]))
                .expect("invalid reactive property source");
        };
        self.push_layout(LayoutOp::SetRotation3D(DVec3::new(x, y, z)))
    }

    /// Turn the drawable so its local x axis points along `x_axis` and its
    /// local y axis along `y_axis`, squared to it, both world directions:
    /// the orientation `rotate_to_3d` would give, without its angles. Its
    /// local z axis, the side a 2D drawable faces, ends along
    /// `x_axis × y_axis`.
    pub fn orient_3d(self, x_axis: DVec3, y_axis: DVec3) -> Result<Self, InvalidOrientation> {
        let rotation =
            gaanim_math::orientation_from_axes(x_axis, y_axis).ok_or(InvalidOrientation)?;
        let (x, y, z) = rotation.to_euler(EulerRot::XYZ);
        Ok(self.rotate_to_3d(x, y, z))
    }

    /// Set the scene-space pivot used by rotations, scales and skews.
    ///
    /// Without one, shapes declared in scene coordinates (lines, polygons,
    /// arcs, arrows, curves) turn about their box center, and other shapes
    /// about their origin. Once the drawable has been shown, the pivot
    /// changes at the cursor, without moving it.
    ///
    /// This is the natural way to rotate a mechanism around a known hinge or
    /// disk center. The engine converts the point to the group's local anchor
    /// after all initial layout has been resolved.
    pub fn with_pivot(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::SetPivot(DVec3::new(x, y, 0.0)))
    }

    pub fn with_pivot_3d(self, x: f64, y: f64, z: f64) -> Self {
        self.push_layout(LayoutOp::SetPivot(DVec3::new(x, y, z)))
    }

    /// Alias for [`Self::with_pivot`].
    pub fn pivot(self, x: f64, y: f64) -> Self {
        self.with_pivot(x, y)
    }

    pub fn at_anchor(self, x: f64, y: f64, anchor: Anchor) -> Self {
        self.push_layout(LayoutOp::MoveAnchorTo {
            target: DVec3::new(x, y, 0.0),
            anchor,
        })
    }

    /// Place this drawable's center at an anchor point derived from another drawable.
    pub fn at_anchor_point(self, point: AnchorPoint) -> Self {
        assert!(
            self.owns_anchor_point(point),
            "anchor point must belong to the same Scene"
        );
        self.push_layout(LayoutOp::MoveToAnchorPoint { point })
    }

    /// Place a text baseline anchor at `(x, y)`.
    pub fn at_text_anchor(self, x: f64, y: f64, anchor: TextAnchor) -> Self {
        self.push_layout(LayoutOp::MoveTextAnchorTo {
            target: DVec3::new(x, y, 0.0),
            anchor,
            center_multiline: false,
        })
    }

    /// Default Text positioning: baseline-center for one line and visual
    /// center for a multiline block.
    pub fn at_text_default(self, x: f64, y: f64) -> Self {
        self.push_layout(LayoutOp::MoveTextAnchorTo {
            target: DVec3::new(x, y, 0.0),
            anchor: TextAnchor::BaselineCenter,
            center_multiline: true,
        })
    }

    pub fn next_to(self, reference: &DrawableHandle, direction: Direction, spacing: f64) -> Self {
        self.next_to_aligned(reference, direction, spacing, Anchor::Center)
    }

    pub fn next_to_aligned(
        self,
        reference: &DrawableHandle,
        direction: Direction,
        spacing: f64,
        aligned_edge: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::NextTo {
            reference: reference.id,
            direction,
            spacing,
            aligned_edge,
        })
    }

    pub fn align_to(
        self,
        reference: &DrawableHandle,
        target_anchor: Anchor,
        reference_anchor: Anchor,
    ) -> Self {
        self.push_layout(LayoutOp::AlignTo {
            reference: reference.id,
            target_anchor,
            reference_anchor,
        })
    }

    pub fn to_edge(self, direction: Direction, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToEdge { direction, buff })
    }

    pub fn to_corner(self, corner: Anchor, buff: f64) -> Self {
        self.push_layout(LayoutOp::ToCorner { corner, buff })
    }

    // -- Internal helpers --

    fn anim(&self, ty: AnimationType) -> Anim {
        if matches!(
            &ty,
            AnimationType::TranslateBy { .. }
                | AnimationType::TranslateTo { .. }
                | AnimationType::TranslateAnchorTo { .. }
                | AnimationType::TranslateToAnchorPoint { .. }
        ) {
            let mut spec = self.spec.lock().expect("object spec poisoned");
            assert!(
                spec.layout_owner.is_none(),
                "a box owns this drawable's position; use .item(offset=...) or detach it"
            );
            spec.manual_position_animation = true;
        }
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx)
    }

    fn anim_dur(&self, ty: AnimationType, dur: Option<f64>) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(self.id, ty, self.state.clone(), active_idx).with_duration(dur)
    }

    pub(crate) fn surrounding_rect_retarget(
        &self,
        from: Vec<crate::anim::BoundsTarget>,
        to: Vec<crate::anim::BoundsTarget>,
        duration: Option<f64>,
    ) -> Anim {
        self.anim_dur(
            AnimationType::SurroundingRectRetarget { from, to },
            duration,
        )
    }

    /// Animates a `ValueTracker` to `to`. Regular drawables ignore this lens.
    pub fn animate_value_to(&self, to: f64) -> Anim {
        self.anim(AnimationType::SignalFloat { to })
    }

    // -- Animation methods (return Anim, auto-enqueued) --
    //
    // Every method accepts an optional duration via `impl OptDuration`:
    //   obj.fade_in()        — default 1.0s
    //   obj.fade_in(2.0)     — 2.0s
    //   obj.fade_in(None)    — default 1.0s

    /// Build one typed animation that can target several drawable properties.
    /// The first property modifier activates the usual deferred auto-queue.
    pub fn animate(&self) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::properties(self.id, self.state.clone(), active_idx, self.spec.clone())
            .with_morph_targets(Arc::clone(&self.morph_targets))
    }

    pub(crate) fn r#move(&self, dx: f64, dy: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, 0.0),
        })
    }

    pub(crate) fn move_to_anim(&self, x: f64, y: f64) -> Anim {
        self.move_to_anchor(x, y, Anchor::Center)
    }

    /// Animate so the selected local bounds anchor reaches `(x, y)`.
    pub(crate) fn move_to_anchor(&self, x: f64, y: f64, anchor: Anchor) -> Anim {
        self.anim(AnimationType::TranslateAnchorTo {
            to: DVec3::new(x, y, 0.0),
            anchor,
        })
    }

    /// Animate this drawable's center to an anchor point on another drawable.
    pub(crate) fn move_to_anchor_point(&self, point: AnchorPoint) -> Anim {
        assert!(
            self.owns_anchor_point(point),
            "anchor point must belong to the same Scene"
        );
        self.anim(AnimationType::TranslateToAnchorPoint { point })
    }

    pub(crate) fn move_3d(&self, dx: f64, dy: f64, dz: f64) -> Anim {
        self.anim(AnimationType::TranslateBy {
            delta: DVec3::new(dx, dy, dz),
        })
    }

    pub(crate) fn move_to_3d_anim(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::TranslateTo {
            to: DVec3::new(x, y, z),
        })
    }

    pub(crate) fn glide_to(&self, x: f64, y: f64) -> Anim {
        self.move_to_anim(x, y)
    }

    pub(crate) fn scale(&self, factor: f64) -> Anim {
        self.anim(AnimationType::ScaleUniform { factor })
    }

    pub(crate) fn scale_to_3d_anim(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::ScaleTo {
            to: DVec3::new(x, y, z),
        })
    }

    pub(crate) fn rotate(&self, rad: f64) -> Anim {
        let pivot = self.spec.lock().ok().and_then(|spec| {
            spec.layout_ops.iter().rev().find_map(|op| match op {
                LayoutOp::SetPivot(p) => Some(*p),
                _ => None,
            })
        });
        self.anim(AnimationType::RotateBy {
            angle_radians: rad,
            pivot,
        })
    }

    pub(crate) fn rotate_by_3d_anim(
        &self,
        axis: &str,
        radians: f64,
    ) -> Result<Anim, RotationAxisError> {
        let delta = match axis.to_ascii_lowercase().as_str() {
            "x" => DQuat::from_rotation_x(radians),
            "y" => DQuat::from_rotation_y(radians),
            "z" => DQuat::from_rotation_z(radians),
            _ => {
                return Err(RotationAxisError {
                    axis: axis.to_owned(),
                });
            }
        };
        Ok(self.anim(AnimationType::RotateBy3D { delta }))
    }

    /// Animate to an XYZ Euler orientation in radians.
    pub(crate) fn rotate_to_3d_anim(&self, x: f64, y: f64, z: f64) -> Anim {
        self.anim(AnimationType::RotateTo {
            to: DQuat::from_euler(EulerRot::XYZ, x, y, z),
        })
    }

    pub(crate) fn rotate_about_point(&self, x: f64, y: f64, rad: f64) -> Anim {
        self.anim(AnimationType::RotateBy {
            angle_radians: rad,
            pivot: Some(DVec3::new(x, y, 0.0)),
        })
    }

    pub(crate) fn fade_in(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeIn, dur.into_opt())
    }

    /// Appears while moving from `direction` toward its final position.
    pub(crate) fn fade_in_from(
        &self,
        direction: Direction,
        distance: f64,
        dur: impl OptDuration,
    ) -> Anim {
        self.anim_dur(
            AnimationType::FadeInFrom {
                offset: direction.to_vector() * distance.max(0.0),
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn fade_out(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::FadeOut, dur.into_opt())
    }

    pub(crate) fn fade_to(&self, alpha: f32) -> Anim {
        self.anim(AnimationType::FadeTo { to: alpha })
    }

    pub(crate) fn write(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Write {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn create(&self, dur: impl OptDuration) -> Anim {
        if matches!(
            &self.spec.lock().expect("object spec poisoned").kind,
            SpawnKind::Primitive3D(..)
        ) {
            return self.anim_dur(AnimationType::Create3D, dur.into_opt());
        }
        self.anim_dur(
            AnimationType::Create {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn unwrite(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Unwrite {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn uncreate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Uncreate {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    /// Sweep this pie or donut slice of a chart from `from` to `to`.
    pub(crate) fn arc_sweep(&self, from: gaanim_math::ArcSlice, to: gaanim_math::ArcSlice) -> Anim {
        self.anim(AnimationType::ArcSweepTo { from, to })
    }

    pub(crate) fn grow_from_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::GrowFromCenter, dur.into_opt())
    }

    pub(crate) fn shrink_to_center(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::ShrinkToCenter, dur.into_opt())
    }

    pub(crate) fn spin_in_from_nothing(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(AnimationType::SpinInFromNothing, dur.into_opt())
    }

    pub(crate) fn grow_from_point(&self, px: f64, py: f64) -> Anim {
        self.anim(AnimationType::GrowFromPoint { px, py })
    }

    pub(crate) fn grow_from_edge(&self, direction: Direction) -> Anim {
        self.anim(AnimationType::GrowFromEdge {
            direction: direction.to_vector(),
        })
    }

    pub(crate) fn draw_border_then_fill(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::DrawBorderThenFill {
                config: DrawAnimationConfig::default(),
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn indicate(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Indicate {
                color: None,
                scale_factor: crate::anim::DEFAULT_INDICATE_SCALE,
            },
            dur.into_opt(),
        )
    }

    pub(crate) fn wiggle(&self, dur: impl OptDuration) -> Anim {
        self.anim_dur(
            AnimationType::Wiggle {
                amplitude: crate::anim::DEFAULT_WIGGLE_AMPLITUDE,
                count: crate::anim::DEFAULT_WIGGLE_COUNT,
            },
            dur.into_opt(),
        )
    }

    /// Reveal a sliding window along this path over a finite timeline clip.
    pub(crate) fn show_passing_flash(&self, duration: f64, time_width: f64) -> Anim {
        self.anim_dur(
            AnimationType::ShowPassingFlash {
                time_width: time_width.clamp(f64::EPSILON, 1.0),
            },
            Some(duration.max(f64::EPSILON)),
        )
    }

    pub(crate) fn move_along_path(&self, path: BezPath) -> Anim {
        self.anim(AnimationType::MoveAlongPath {
            path,
            path_target: None,
            follow: Default::default(),
        })
    }

    pub(crate) fn move_along_path_3d(&self, points: Vec<gaanim_core::glam::DVec3>) -> Anim {
        self.anim(AnimationType::MoveAlongPath3D { points })
    }

    pub(crate) fn move_along_drawable(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::MoveAlongPath {
            path: BezPath::new(),
            path_target: Some(target.id),
            follow: Default::default(),
        })
    }

    pub(crate) fn fade_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::FadeTransform { target: target.id })
    }

    pub(crate) fn transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::Transform {
            target: target.id,
            pairs: Vec::new(),
        })
    }

    pub(crate) fn replacement_transform(&self, target: &DrawableHandle) -> Anim {
        self.anim(AnimationType::ReplacementTransform { target: target.id })
    }

    // -- Reactive methods --

    /// Attach a preset updater that runs every frame.
    ///
    /// Use `UpdaterPreset` variants: `Orbit`, `AdvanceX`, `Bob`, `Rotate`, `Pulse`.
    /// `Bob`, `Rotate` and `Pulse` attach as procedural layers; see
    /// [`UpdaterPreset::procedural`].
    pub fn add_updater(&self, preset: UpdaterPreset) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachUpdater {
                target: self.id,
                preset: preset.procedural(),
            });
    }

    /// Queue a custom updater while retaining it with the deferred canvas.
    ///
    /// This is primarily used by language bindings. Keeping the updater in the
    /// operation makes repeated canvas compilation (preview followed by export)
    /// deterministic and avoids process-global callback registries.
    #[doc(hidden)]
    pub fn add_custom_updater(&self, updater: gaanim_animation::Updater) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachCustomUpdater {
                target: self.id,
                updater,
            });
    }

    /// Remove any updater attached to this entity.
    pub fn remove_updater(&self) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::RemoveUpdater(self.id));
    }

    /// Drive a channel from a falloff, for this drawable or, when it is a
    /// group, for each of its members, from the cursor on.
    pub fn drive_falloff(&self, effect: gaanim_animation::FalloffEffect) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachFalloff {
                target: self.id,
                effect,
            });
    }

    /// End the falloff effects of this drawable or of its group's members.
    pub fn clear_falloff(&self) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::ClearFalloff(self.id));
    }

    /// Copy the source entity's Y position each frame (after updaters run).
    pub fn bind_y_from(&self, source: &DrawableHandle) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes: AxisMask::Y,
            });
    }

    /// Copy the source entity's X position each frame (after updaters run).
    pub fn bind_x_from(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::X);
    }

    /// Keep this drawable centered on `source` each frame. The drawable is
    /// deferred until an entry animation is included in `SceneModel::play`.
    ///
    /// This is an exact XY position binding. It is useful for labels, markers,
    /// and accents that should travel with an independently animated object.
    pub fn attach_to(&self, source: &DrawableHandle) {
        self.bind_position_from(source, AxisMask::XY);
    }

    /// Follow `source` while preserving a scene-space `(x, y)` offset. The
    /// drawable is deferred until an entry animation is included in
    /// `SceneModel::play`.
    pub fn follow_to(&self, source: &DrawableHandle, offset_x: f64, offset_y: f64) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionFollow {
                target: self.id,
                source: source.id,
                offset: DVec3::new(offset_x, offset_y, 0.0),
            });
    }

    /// Follow any reactive endpoint with an offset expressed in world or source-local axes.
    pub fn follow_endpoint(
        &self,
        endpoint: crate::canvas::CanvasEndpoint,
        offset: DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
    ) -> Self {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachEndpointFollow {
                target: self.id,
                endpoint,
                offset,
                offset_space,
            });
        self.clone()
    }

    /// Follow `endpoint` as it was `delay` seconds earlier (After Effects'
    /// `valueAtTime(time - delay)`), for trails and tails. The timeline
    /// evaluates the leader's own animations at `t - delay` after every
    /// seek, so the trail needs no history; until the current segment has
    /// run for `delay` seconds the follower holds the leader's position at
    /// the segment start. A `delay` of 0 is [`Self::follow_endpoint`]; a
    /// positive one needs a drawable endpoint.
    pub fn follow_endpoint_delayed(
        &self,
        endpoint: crate::canvas::CanvasEndpoint,
        offset: DVec3,
        offset_space: gaanim_animation::FollowOffsetSpace,
        delay: f64,
    ) -> Result<Self, String> {
        if !(delay.is_finite() && delay >= 0.0) {
            return Err(format!("delay must be a finite number >= 0, got {delay}"));
        }
        if delay == 0.0 {
            return Ok(self.follow_endpoint(endpoint, offset, offset_space));
        }
        let crate::canvas::CanvasEndpoint::Entity(source) = endpoint else {
            return Err("a delayed follow needs a drawable to follow".to_string());
        };
        if source == self.id {
            return Err("a drawable cannot follow itself".to_string());
        }
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachDelayedFollow {
                target: self.id,
                source,
                delay,
                offset,
                offset_space,
            });
        Ok(self.clone())
    }

    /// Couple this drawable's world Z rotation to another drawable.
    pub fn bind_rotation_from(&self, source: &DrawableHandle, ratio: f64, phase: f64) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachRotationBinding {
                target: self.id,
                source: source.id,
                ratio,
                phase,
            });
        self.clone()
    }

    /// Translate this drawable along a scene axis from a source drawable's rotation.
    pub fn bind_translation_from_rotation(
        &self,
        source: &DrawableHandle,
        axis: DVec3,
        scale: f64,
    ) -> Self {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachRotationTranslationBinding {
                target: self.id,
                source: source.id,
                axis,
                scale,
            });
        self.clone()
    }

    /// Drive a property of this drawable along a sampled `(times, values)`
    /// series, evaluated natively as a pure function of timeline time.
    ///
    /// Translation axes and Z rotation are relative to the authored pose
    /// (captured lazily on first evaluation): `base + offset + scale * sample`.
    /// Uniform scale, opacity, and float signals are absolute:
    /// `offset + scale * sample`. Seeks and paused scrubbing are exact because
    /// the driver accumulates no state. The series starts at the cursor;
    /// before it, the channel keeps what earlier animations give it. Call
    /// `remove_updater` to detach it.
    pub fn drive_from_samples(
        &self,
        times: Vec<f64>,
        values: Vec<f64>,
        property: SampledProperty,
        interpolation: SampledInterpolation,
        scale: f64,
        offset: f64,
    ) -> Result<Self, InvalidSampledSeries> {
        let driver =
            SampledSeriesDriver::new(times, values, property, interpolation, scale, offset)?;
        Ok(self.attach_sampled_series(driver))
    }

    /// Like [`drive_from_samples`](Self::drive_from_samples), but translation
    /// and Z rotation samples are absolute, `offset + scale * sample`, in the
    /// parent's coordinates (the scene's for a top-level drawable) instead of
    /// offsets from the authored pose.
    pub fn drive_absolute_from_samples(
        &self,
        times: Vec<f64>,
        values: Vec<f64>,
        property: SampledProperty,
        interpolation: SampledInterpolation,
        scale: f64,
        offset: f64,
    ) -> Result<Self, InvalidSampledSeries> {
        let driver =
            SampledSeriesDriver::new(times, values, property, interpolation, scale, offset)?
                .absolute();
        Ok(self.attach_sampled_series(driver))
    }

    /// Drive each of `targets` along one sampled series at its own scale,
    /// as [`Self::drive_from_samples`] (or, with `absolute`,
    /// [`Self::drive_absolute_from_samples`]) drives one drawable: the
    /// times and values are validated and kept once, and shared by every
    /// target. A non-finite scale is rejected before any target is driven.
    pub fn drive_each_from_samples(
        targets: &[(&DrawableHandle, f64)],
        times: Vec<f64>,
        values: Vec<f64>,
        property: SampledProperty,
        interpolation: SampledInterpolation,
        offset: f64,
        absolute: bool,
    ) -> Result<(), InvalidSampledSeries> {
        if targets.iter().any(|(_, scale)| !scale.is_finite()) {
            return Err(InvalidSampledSeries);
        }
        let mut driver =
            SampledSeriesDriver::new(times, values, property, interpolation, 1.0, offset)?;
        if absolute {
            driver = driver.absolute();
        }
        for (target, scale) in targets {
            target.attach_sampled_series(SampledSeriesDriver {
                scale: *scale,
                ..driver.clone()
            });
        }
        Ok(())
    }

    pub(crate) fn attach_sampled_series(&self, driver: SampledSeriesDriver) -> Self {
        let driven = matches!(driver.property, SampledProperty::Signal).then(|| {
            driver
                .values
                .iter()
                .map(|value| value * driver.scale + driver.offset)
                .collect::<Vec<_>>()
        });
        let mut state = self.state.lock().expect("canvas state poisoned");
        if let Some(values) = driven {
            state.widen_parameter_range(self.id, values);
        }
        state.active_mut().ops.push(Op::AttachSampledSeries {
            target: self.id,
            driver,
        });
        self.clone()
    }

    /// Turn this drawable to a sampled series of 3D orientations: at each of
    /// `times` (seconds from the cursor), the quaternion of the same index,
    /// absolute in its parent's frame. Between samples `Linear` turns the
    /// shorter way (slerp); before the first and after the last it holds
    /// them. Evaluated natively as a pure function of timeline time, like
    /// [`drive_from_samples`](Self::drive_from_samples).
    pub fn drive_orientation_from_samples(
        &self,
        times: Vec<f64>,
        orientations: Vec<DQuat>,
        interpolation: SampledInterpolation,
    ) -> Result<Self, InvalidSampledSeries> {
        let driver = SampledSeriesDriver::orientations(times, orientations, interpolation)?;
        Ok(self.attach_sampled_series(driver))
    }

    /// Color the fill of this drawable, and of its members with a fill, from
    /// a sampled `(times, values)` series, evaluated natively like
    /// [`drive_from_samples`](Self::drive_from_samples). The value
    /// `offset + scale * sample` is read in `domain` (the series' range when
    /// `None`) and picks a color among `colors`, evenly spaced.
    #[allow(clippy::too_many_arguments)]
    pub fn drive_fill_from_samples(
        &self,
        times: Vec<f64>,
        values: Vec<f64>,
        colors: Vec<Color>,
        domain: Option<(f64, f64)>,
        interpolation: SampledInterpolation,
        scale: f64,
        offset: f64,
    ) -> Result<Self, InvalidSampledSeries> {
        let driver = SampledSeriesDriver::new(
            times,
            values,
            SampledProperty::Fill,
            interpolation,
            scale,
            offset,
        )?
        .with_colors(colors, domain)?;
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachSampledSeries {
                target: self.id,
                driver,
            });
        Ok(self.clone())
    }

    /// Copy the source entity's position on specified axes each frame.
    pub fn bind_position_from(&self, source: &DrawableHandle, axes: AxisMask) {
        self.defer_visibility_until_play();
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::AttachPositionBinding {
                target: self.id,
                source: source.id,
                axes,
            });
    }
}

impl FragmentSelection {
    pub fn bounds_target(&self) -> crate::anim::BoundsTarget {
        crate::anim::BoundsTarget::TextSelection {
            target: self.target,
            fragment: self.fragment.clone(),
            occurrence: self.occurrence,
        }
    }

    fn push(&self, op: Op) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(op);
    }

    fn animate(self, effect: TextSelectionEffect, duration: impl OptDuration) -> Anim {
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::queued(
            self.target,
            AnimationType::TextSelection {
                fragment: self.fragment,
                occurrence: self.occurrence,
                effect,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration)
    }

    /// Build a compound fill/opacity animation scoped to this selection.
    pub fn animate_properties(self) -> Anim {
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Anim::text_selection_properties(
            self.target,
            self.fragment,
            self.occurrence,
            self.state,
            active_idx,
        )
    }

    /// Instantly colors this selected fragment: its declared color before
    /// the text is first shown, a change at the cursor afterwards.
    pub fn fill(self, color: Color) -> Self {
        if self.fragment.trim().is_empty() {
            return self;
        }
        let mut state = self.state.lock().expect("canvas state poisoned");
        if state.frozen_spawn_specs.contains_key(&self.target) {
            let anim_type = AnimationType::TextSelectionProperties {
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                properties: PropertyAnimation {
                    fill: Some(Brush::Solid(color)),
                    ..PropertyAnimation::default()
                },
            };
            let rate_func = anim_type.default_rate_func();
            // Not merged with other immediate changes: another selection of
            // the same text is another channel.
            state.active_mut().ops.push(Op::Immediate(AnimationBuilder {
                target: self.target,
                anim_type,
                duration: 0.0,
                delay: 0.0,
                rate_func,
            }));
            drop(state);
            return self;
        }
        drop(state);
        self.push(Op::FragmentFill {
            target: self.target,
            fragment: self.fragment.clone(),
            occurrence: self.occurrence,
            color,
        });
        self
    }

    /// Emphasizes this fragment without affecting the surrounding text.
    pub fn indicate(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Indicate, duration)
    }

    pub fn pulse(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Pulse, duration)
    }

    pub fn wiggle(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Wiggle, duration)
    }

    pub fn wave(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Wave, duration)
    }

    pub fn highlight(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Highlight, duration)
    }

    pub fn focus(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Focus, duration)
    }

    pub fn brace(self, label: String, above: bool, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Brace { label, above }, duration)
    }

    pub fn annotate(self, label: String, offset: DVec3, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Annotate { label, offset }, duration)
    }

    /// Sweeps a highlighter band behind each selected line, left to right.
    pub fn marker(self, style: crate::anim::TextMarkerStyle, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Marker(style), duration)
    }

    /// Instantly places a highlighter band behind each selected line at the
    /// current timeline cursor.
    pub fn with_marker(self, style: crate::anim::TextMarkerStyle) -> Self {
        if !self.fragment.trim().is_empty() {
            let effect = TextSelectionEffect::Marker(style);
            let anim_type = AnimationType::TextSelection {
                fragment: self.fragment.clone(),
                occurrence: self.occurrence,
                effect,
            };
            let rate_func = anim_type.default_rate_func();
            self.push(Op::Immediate(AnimationBuilder {
                target: self.target,
                anim_type,
                duration: 0.0,
                delay: 0.0,
                rate_func,
            }));
        }
        self
    }

    /// Reveals this fragment with `Fade`, `Wipe`, or `FromBelow`.
    pub fn reveal(self, style: FragmentRevealStyle, duration: impl OptDuration) -> Anim {
        let effect = match style {
            FragmentRevealStyle::Fade => TextSelectionEffect::RevealFade,
            FragmentRevealStyle::Wipe => TextSelectionEffect::RevealWipe,
            FragmentRevealStyle::FromBelow => TextSelectionEffect::RevealFromBelow,
        };
        self.animate(effect, duration)
    }

    /// Strikes through this fragment and fades it from the equation.
    pub fn cancel(self, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::Cancel, duration)
    }

    /// Animates this fragment's fill to `color`.
    pub fn color_to(self, color: Color, duration: impl OptDuration) -> Anim {
        self.animate(TextSelectionEffect::ColorTo(color), duration)
    }

    /// Animates this fragment's opacity to `opacity`.
    pub fn opacity_to(self, opacity: f32, duration: impl OptDuration) -> Anim {
        self.animate(
            TextSelectionEffect::OpacityTo(opacity.clamp(0.0, 1.0)),
            duration,
        )
    }

    /// Morphs this selection into `target`, pairing matching glyphs in order.
    /// Unpaired glyphs remain unchanged; use equal-sized fragments for the
    /// clearest equation derivations.
    pub fn morph_to(
        self,
        target: &FragmentSelection,
        duration: impl OptDuration,
    ) -> Result<Anim, LayoutOwnershipError> {
        if !Arc::ptr_eq(&self.state, &target.state) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        if self.layout_owner != target.layout_owner
            && (self.layout_owner.is_some() || target.layout_owner.is_some())
        {
            return Err(LayoutOwnershipError::AlreadyManaged {
                owner: target
                    .layout_owner
                    .or(self.layout_owner)
                    .expect("one owner exists"),
            });
        }
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            self.target,
            AnimationType::TextSelectionTransform {
                target: target.target,
                source_fragment: self.fragment,
                source_occurrence: self.occurrence,
                target_fragment: target.fragment.clone(),
                target_occurrence: target.occurrence,
                copy: false,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration))
    }

    /// Copies this selection to `target` while keeping both parent texts
    /// visible. This is the structured-text replacement for equation-term
    /// copying.
    pub fn copy_to(
        self,
        target: &FragmentSelection,
        duration: impl OptDuration,
    ) -> Result<Anim, LayoutOwnershipError> {
        if !Arc::ptr_eq(&self.state, &target.state) {
            return Err(LayoutOwnershipError::ForeignScene);
        }
        if self.layout_owner != target.layout_owner
            && (self.layout_owner.is_some() || target.layout_owner.is_some())
        {
            return Err(LayoutOwnershipError::AlreadyManaged {
                owner: target
                    .layout_owner
                    .or(self.layout_owner)
                    .expect("one owner exists"),
            });
        }
        let duration = duration.into_opt();
        let active_idx = self.state.lock().expect("canvas state poisoned").active_idx;
        Ok(Anim::queued(
            self.target,
            AnimationType::TextSelectionTransform {
                target: target.target,
                source_fragment: self.fragment,
                source_occurrence: self.occurrence,
                target_fragment: target.fragment.clone(),
                target_occurrence: target.occurrence,
                copy: true,
            },
            self.state,
            active_idx,
        )
        .with_duration(duration))
    }
}

/// Pairs the semantic parts two texts share by name (or the explicitly
/// `requested` name pairs) as `(source fragment, occurrence, target fragment,
/// occurrence)` for [`AnimationType::TextTransition`].
pub(crate) fn text_semantic_pairs(
    source_tags: &[(String, String, Option<usize>)],
    target_tags: &[(String, String, Option<usize>)],
    requested: Option<Vec<(String, String)>>,
) -> Vec<(String, Option<usize>, String, Option<usize>)> {
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
                .find(|(name, _, _)| name == &source_name)?;
            let (_, target_fragment, target_occurrence) = target_tags
                .iter()
                .rev()
                .find(|(name, _, _)| name == &target_name)?;
            Some((
                source_fragment.clone(),
                *source_occurrence,
                target_fragment.clone(),
                *target_occurrence,
            ))
        })
        .collect()
}
