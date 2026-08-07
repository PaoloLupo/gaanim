//! Deferred operations and segment tracking for the Canvas API.

use std::sync::{Arc, Mutex};

use gaanim_animation::AxisMask;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::Color;
use gaanim_timeline::transition::TransitionType;

use crate::anim::AnimationBuilder;
use crate::canvas::SlideId;
use crate::canvas::types::{LayoutKind, ObjectSpec};

// -----------------------------------------------------------------------
// Shared state
// -----------------------------------------------------------------------

/// Mutable state shared by a [`Canvas`](super::Canvas) and the handles it
/// creates. This lets fluent object setters and auto-queued animations update
/// the same deferred operation stream that will later be compiled.
#[derive(Debug)]
pub(crate) struct CanvasState {
    pub segments: Vec<Segment>,
    pub active_idx: usize,
    pub next_id: u32,
    pub all_drawables: Vec<ObjectId>,
}

impl CanvasState {
    pub fn new() -> Self {
        Self {
            segments: vec![Segment::new("_default")],
            active_idx: 0,
            next_id: 1,
            all_drawables: Vec::new(),
        }
    }

    pub fn active(&self) -> &Segment {
        &self.segments[self.active_idx]
    }

    pub fn active_mut(&mut self) -> &mut Segment {
        &mut self.segments[self.active_idx]
    }

    pub fn next_object_id(&mut self) -> ObjectId {
        let id = self.next_id;
        self.next_id += 1;
        ObjectId::from_parts(id, 1)
    }
}

pub(crate) type SharedCanvasState = Arc<Mutex<CanvasState>>;
pub(crate) type SharedObjectSpec = Arc<Mutex<ObjectSpec>>;

// -----------------------------------------------------------------------
// Op
// -----------------------------------------------------------------------

/// A deferred operation accumulated by [`Canvas`](super::Canvas) and replayed
/// into a [`SceneBuilder`](crate::builder::SceneBuilder) on compile.
#[derive(Debug, Clone)]
pub(crate) enum Op {
    /// Spawn a mobject from a shared ObjectSpec. The spec is intentionally
    /// shared so fluent setters after factory creation update the spawned
    /// object seen by compile.
    Spawn(SharedObjectSpec),
    /// Play a single animation sequentially (auto-queued). `active=false`
    /// means the animation was later regrouped by `Canvas::play(...)`.
    Animate {
        anim: AnimationBuilder,
        active: bool,
    },
    /// Play several animations in parallel.
    Play(Vec<AnimationBuilder>),
    /// Set the fill of selected glyphs after their textual hierarchy exists.
    FragmentFill {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        color: Color,
    },
    /// Emphasize selected glyphs as a parallel animation.
    FragmentIndicate {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        color: Option<Color>,
        duration: f64,
    },
    /// Reveal a selected fragment with a presentation-oriented preset.
    FragmentReveal {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        style: FragmentRevealStyle,
        duration: f64,
    },
    /// Draw a strikethrough across a selected fragment while fading it out.
    CancelFragment {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        duration: f64,
    },
    BraceLabel {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        label: String,
        above: bool,
        duration: f64,
    },
    AnnotateFragment {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        label: String,
        offset: DVec3,
        duration: f64,
    },
    /// Write semantic equation terms one after another. Each tuple is one
    /// term, even when it resolves to several glyphs.
    WriteTerms {
        target: ObjectId,
        terms: Vec<(String, Option<usize>)>,
        duration: f64,
    },
    /// Dim every equation glyph except the selected semantic terms, then
    /// pulse the selected terms.
    FocusEquation {
        target: ObjectId,
        terms: Vec<(String, Option<usize>)>,
        dim_opacity: f32,
        duration: f64,
    },
    /// Tween selected glyph fills to a color in parallel.
    FragmentFillTo {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        color: Color,
        duration: f64,
    },
    /// Morph selected source glyphs into selected target glyphs pairwise.
    FragmentTransform {
        source: ObjectId,
        source_fragment: String,
        source_occurrence: Option<usize>,
        target: ObjectId,
        target_fragment: String,
        target_occurrence: Option<usize>,
        duration: f64,
    },
    /// Morph every explicitly paired semantic equation tag in parallel.
    TaggedTransform {
        source: ObjectId,
        target: ObjectId,
        pairs: Vec<(String, Option<usize>, String, Option<usize>)>,
        duration: f64,
    },
    /// Replace one equation state with another while using a semantic tag as
    /// the moving anchor. Remaining target glyphs fade in as the source fades
    /// out, which makes single terms naturally expand into longer expressions.
    ExpandEquation {
        source: ObjectId,
        target: ObjectId,
        source_fragment: String,
        source_occurrence: Option<usize>,
        target_fragment: String,
        target_occurrence: Option<usize>,
        duration: f64,
    },
    /// Transition between equation steps by matching their common glyphs.
    StepEquation {
        source: ObjectId,
        target: ObjectId,
        duration: f64,
    },
    /// Auto-match and morph between two objects — improved TransformMatchingShapes/Tex.
    TransformMatching {
        source: ObjectId,
        target: ObjectId,
        mode: String,
        duration: f64,
    },
    /// Advance the cursor by a duration (no animation).
    Wait(f64),
    CameraPosition {
        to: DVec3,
        duration: f64,
    },
    CameraZoom {
        to: f64,
        duration: f64,
    },
    CameraRotation {
        to: DQuat,
        duration: f64,
    },
    CameraFrame {
        target: ObjectId,
        margin: f64,
        duration: f64,
    },
    CameraFollow {
        target: ObjectId,
        duration: f64,
    },
    CameraShake {
        amplitude: f64,
        frequency: f64,
        duration: f64,
    },
    CameraLookAt {
        eye: DVec3,
        target: DVec3,
        up: DVec3,
        duration: f64,
    },
    CameraOrbit {
        delta_yaw: f64,
        delta_pitch: f64,
        duration: f64,
    },
    CameraPerspective {
        fov_y: f64,
        near: f64,
        far: f64,
        duration: f64,
    },
    CameraDolly {
        factor: f64,
        duration: f64,
    },
    /// Clip a drawable hierarchy by another drawable's vector geometry.
    SetClip {
        target: ObjectId,
        mask: Option<ObjectId>,
        rule: gaanim_core::peniko::Fill,
    },
    /// Insert a slide breakpoint.
    Slide,
    /// Begin a semantic slide. Unlike [`Self::Slide`], this is not a
    /// breakpoint: it scopes the objects that follow so the compiler can keep
    /// future slides hidden and retire the preceding slide automatically.
    PresentationSlideStart(SlideId),
    /// Set an object visible (instant).
    Show(ObjectId),
    /// Set an object invisible (instant).
    Hide(ObjectId),
    /// Remove an object completely.
    Remove(ObjectId),
    /// Recompute a layout container and optionally animate every affected child
    /// into its new position.
    LayoutReflow {
        container: ObjectId,
        members: Vec<ObjectId>,
        kind: LayoutKind,
        gap: f64,
        duration: Option<f64>,
        entering: Option<ObjectId>,
        leaving: Option<ObjectId>,
        max_width: Option<f64>,
        max_height: Option<f64>,
        shrink_to_fit: bool,
        wrap: bool,
        justify: String,
    },

    // -- Reactive ops (Phase 2) --
    /// Attach a preset updater to an existing entity.
    AttachUpdater {
        target: ObjectId,
        preset: UpdaterPreset,
    },
    /// Remove the updater from an entity.
    RemoveUpdater(ObjectId),
    /// Attach a TracedPath to an entity, tracking a source entity's movement.
    AttachTracedPath {
        target: ObjectId,
        source: ObjectId,
        min_distance: f64,
        max_points: Option<usize>,
    },
    /// Attach a PositionBinding — copy source axes to target each frame.
    AttachPositionBinding {
        target: ObjectId,
        source: ObjectId,
        axes: AxisMask,
    },
    /// Follow a source entity while retaining a scene-space offset.
    AttachPositionFollow {
        target: ObjectId,
        source: ObjectId,
        offset: DVec3,
    },
    /// Attach a TrackingLine — reactive line between two endpoints.
    AttachTrackingLine {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
    },
    /// Attach a spring whose endpoints follow entities or fixed positions.
    AttachTrackingSpring {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
    },
    /// Attach a dynamic dimension line between two endpoints.
    AttachTrackingDimension {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
    },
    /// Regenerate a curved arrow arc from a float signal every frame.
    AttachTrackerArc {
        target: ObjectId,
        tracker: ObjectId,
        center: (f64, f64),
        radius: f64,
        start_angle: f64,
        sweep_scale: f64,
        sweep_offset: f64,
    },
    /// Keep a point drawable at a normalized arc-length along a sampled curve.
    AttachPointOnCurve {
        target: ObjectId,
        curve: ObjectId,
        tracker: ObjectId,
    },
    /// Keep a line centered and tangent to a sampled curve.
    AttachTangentOnCurve {
        target: ObjectId,
        curve: ObjectId,
        tracker: ObjectId,
    },
    /// Keep a line centered and normal to a sampled curve.
    AttachNormalOnCurve {
        target: ObjectId,
        curve: ObjectId,
        tracker: ObjectId,
    },
    AttachCurvatureOnCurve {
        target: ObjectId,
        curve: ObjectId,
        tracker: ObjectId,
        window: f64,
    },
}

/// Visual preset used by [`Op::FragmentReveal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentRevealStyle {
    Fade,
    Wipe,
    FromBelow,
}

/// A tracking endpoint at the Canvas level (before entity resolution).
#[derive(Debug, Clone)]
pub enum CanvasEndpoint {
    /// Fixed position in space.
    Static(DVec3),
    /// Position follows an entity (referenced by Canvas ObjectId).
    Entity(ObjectId),
}

/// Preset updater types that can be attached to entities via the Canvas API.
#[derive(Debug, Clone)]
pub enum UpdaterPreset {
    /// Orbit around a center point at a given radius and angular speed.
    Orbit {
        cx: f64,
        cy: f64,
        radius: f64,
        speed: f64,
    },
    /// Move X by `speed * dt` each frame.
    AdvanceX { speed: f64 },
    /// Sinusoidal Y oscillation.
    Bob { amplitude: f64, frequency: f64 },
    /// Continuous Z-axis rotation.
    Rotate { speed: f64 },
    /// Scale oscillation.
    Pulse {
        min_scale: f64,
        max_scale: f64,
        frequency: f64,
    },
}

impl UpdaterPreset {
    /// Convert this preset into a boxed `Updater` component.
    pub fn into_updater(self) -> gaanim_animation::Updater {
        match self {
            UpdaterPreset::Orbit {
                cx,
                cy,
                radius,
                speed,
            } => gaanim_animation::orbit_updater(DVec3::new(cx, cy, 0.0), radius, speed),
            UpdaterPreset::AdvanceX { speed } => gaanim_animation::advance_x_updater(speed),
            UpdaterPreset::Bob {
                amplitude,
                frequency,
            } => gaanim_animation::bob_updater(amplitude, frequency),
            UpdaterPreset::Rotate { speed } => gaanim_animation::rotate_updater(speed),
            UpdaterPreset::Pulse {
                min_scale,
                max_scale,
                frequency,
            } => gaanim_animation::pulse_updater(min_scale, max_scale, frequency),
        }
    }
}

// -----------------------------------------------------------------------
// Segment
// -----------------------------------------------------------------------

/// A named segment (≈ scene) within a [`Canvas`](super::Canvas).
#[derive(Debug, Clone)]
pub struct Segment {
    pub name: String,
    pub(crate) cursor: f64,
    pub(crate) ops: Vec<Op>,
    /// Transition from the previous segment into this one (if any).
    pub transition: Option<TransitionType>,
    /// Index of the segment that precedes this one.
    pub(crate) prev_segment: Option<usize>,
    /// ObjectIds that belong to this segment.
    pub(crate) mobject_ids: Vec<ObjectId>,
}

impl Segment {
    pub(crate) fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cursor: 0.0,
            ops: Vec::new(),
            transition: None,
            prev_segment: None,
            mobject_ids: Vec::new(),
        }
    }
}
