//! Deferred operations and segment tracking for the SceneModel API.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gaanim_animation::{
    AngleArrowheads, AngleSweep, AxisMask, DimensionLabelOrientation, FollowOffsetSpace,
    SampledSeriesDriver, Updater,
};
use gaanim_animation::{ReactiveFunction, ScalarSource};
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_core::peniko::Color;
use gaanim_layout::LayoutConstraint;
use gaanim_renderer::background::BackgroundPaint;
use gaanim_timeline::transition::TransitionType;

use crate::anim::AnimationBuilder;
use crate::canvas::SegmentId;
use crate::canvas::types::{LayoutTreeSnapshot, ObjectSpec};

// -----------------------------------------------------------------------
// Shared state
// -----------------------------------------------------------------------

/// Mutable state shared by a [`SceneModel`](super::SceneModel) and the handles it
/// creates. This lets fluent object setters and auto-queued animations update
/// the same deferred operation stream that will later be compiled.
#[derive(Debug)]
pub(crate) struct CanvasState {
    pub segments: Vec<Segment>,
    pub active_idx: usize,
    pub next_id: u32,
    pub next_segment_id: u32,
    pub next_camera_binding_order: u64,
    pub next_camera_state_id: u64,
    pub saved_camera_states: HashMap<String, gaanim_animation::CameraStateSource>,
    pub all_drawables: Vec<ObjectId>,
    pub layout_constraints: Vec<LayoutConstraint>,
    pub layout_diagnostics: Vec<(Option<ObjectId>, String)>,
    /// Latest authoritative authoring snapshot for every Layout v2 root.
    /// Python layout handles and width-sensitive leaves share this registry.
    pub latest_layouts: HashMap<ObjectId, LayoutTreeSnapshot>,
    /// Authoring-side mirrors used to materialize initial reactive snapshots.
    pub parameter_values: HashMap<ObjectId, Arc<Mutex<f64>>>,
}

impl CanvasState {
    pub fn new() -> Self {
        Self {
            segments: vec![Segment::implicit()],
            active_idx: 0,
            next_id: 1,
            next_segment_id: 1,
            next_camera_binding_order: 0,
            next_camera_state_id: 1,
            saved_camera_states: HashMap::new(),
            all_drawables: Vec::new(),
            layout_constraints: Vec::new(),
            layout_diagnostics: Vec::new(),
            latest_layouts: HashMap::new(),
            parameter_values: HashMap::new(),
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

    pub fn next_segment_id(&mut self) -> SegmentId {
        let id = self.next_segment_id;
        self.next_segment_id += 1;
        SegmentId(id)
    }

    pub fn next_camera_binding_order(&mut self) -> u64 {
        let order = self.next_camera_binding_order;
        self.next_camera_binding_order += 1;
        order
    }

    pub fn next_camera_state_id(&mut self) -> u64 {
        let id = self.next_camera_state_id;
        self.next_camera_state_id += 1;
        id
    }
}

pub(crate) type SharedCanvasState = Arc<Mutex<CanvasState>>;
pub(crate) type SharedObjectSpec = Arc<Mutex<ObjectSpec>>;

pub(crate) type SharedCameraBindingSpec = Arc<Mutex<CameraBindingSpec>>;

/// Authoring-time activation window for a persistent camera constraint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CameraBindingWindowSpec {
    pub start: f64,
    pub end: Option<f64>,
}

/// Deferred, unresolved channels of a native camera binding.
#[derive(Debug, Clone)]
pub(crate) enum CanvasCameraBindingKind {
    TwoD {
        center: Option<CanvasEndpoint>,
        zoom: Option<ScalarSource>,
        rotation: Option<ScalarSource>,
    },
    ThreeD {
        eye: Option<CanvasEndpoint>,
        target: Option<CanvasEndpoint>,
        fov_y: Option<ScalarSource>,
        up: DVec3,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct CameraBindingSpec {
    pub order: u64,
    pub kind: CanvasCameraBindingKind,
    pub influence: ScalarSource,
    pub windows: Vec<CameraBindingWindowSpec>,
}

// -----------------------------------------------------------------------
// Op
// -----------------------------------------------------------------------

/// A deferred operation accumulated by [`SceneModel`](super::SceneModel) and replayed
/// into a [`SceneBuilder`](crate::builder::SceneBuilder) on compile.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) enum Op {
    /// Spawn a mobject from a shared ObjectSpec. The spec is intentionally
    /// shared so fluent setters after factory creation update the spawned
    /// object seen by compile.
    Spawn(SharedObjectSpec),
    /// Spawn a non-rendered persistent camera binding.
    SpawnCameraBinding(SharedCameraBindingSpec),
    /// Capture the authored camera pose at the current timeline cursor.
    CaptureCameraState { id: u64 },
    /// Play a single animation sequentially (auto-queued). `active=false`
    /// means the animation was later regrouped by `SceneModel::play(...)`.
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
    FragmentIndicate {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        color: Option<Color>,
        duration: f64,
    },
    FragmentEmphasis {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        kind: String,
        duration: f64,
    },
    FragmentReveal {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        style: FragmentRevealStyle,
        duration: f64,
    },
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
    FocusEquation {
        target: ObjectId,
        terms: Vec<(String, Option<usize>)>,
        dim_opacity: f32,
        duration: f64,
    },
    FragmentFillTo {
        target: ObjectId,
        fragment: String,
        occurrence: Option<usize>,
        color: Color,
        duration: f64,
    },
    FragmentTransform {
        source: ObjectId,
        source_fragment: String,
        source_occurrence: Option<usize>,
        target: ObjectId,
        target_fragment: String,
        target_occurrence: Option<usize>,
        duration: f64,
    },
    TaggedTransform {
        source: ObjectId,
        target: ObjectId,
        pairs: Vec<(String, Option<usize>, String, Option<usize>)>,
        duration: f64,
    },
    /// Replace one equation state with another while using a semantic tag as
    /// the moving anchor. Residual glyphs shrink into or grow from their own
    /// visual centers.
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
        pairs: Vec<(String, Option<usize>, String, Option<usize>)>,
        duration: f64,
    },
    /// Auto-match and morph between two objects — improved TransformMatchingShapes/Tex.
    TransformMatching {
        source: ObjectId,
        target: ObjectId,
        mode: String,
        semantic_pairs: Vec<(String, Option<usize>, String, Option<usize>)>,
        duration: f64,
    },
    /// Advance the cursor by a duration (no animation).
    Wait(f64),
    // Retained as a replay compatibility path for previously queued internal
    // camera operations. New camera methods use AnimationType camera payloads.
    #[allow(dead_code)]
    CameraPosition { to: DVec3, duration: f64 },
    #[allow(dead_code)]
    CameraZoom { to: f64, duration: f64 },
    #[allow(dead_code)]
    CameraRotation { to: DQuat, duration: f64 },
    #[allow(dead_code)]
    CameraFrame {
        target: ObjectId,
        margin: f64,
        duration: f64,
    },
    #[allow(dead_code)]
    CameraFollow { target: ObjectId, duration: f64 },
    #[allow(dead_code)]
    CameraShake {
        amplitude: f64,
        frequency: f64,
        duration: f64,
    },
    #[allow(dead_code)]
    CameraLookAt {
        eye: DVec3,
        target: DVec3,
        up: DVec3,
        duration: f64,
    },
    #[allow(dead_code)]
    CameraOrbit {
        delta_yaw: f64,
        delta_pitch: f64,
        duration: f64,
    },
    #[allow(dead_code)]
    CameraPerspective {
        fov_y: f64,
        near: f64,
        far: f64,
        duration: f64,
    },
    #[allow(dead_code)]
    CameraDolly { factor: f64, duration: f64 },
    /// Clip a drawable hierarchy by another drawable's vector geometry.
    SetClip {
        target: ObjectId,
        mask: Option<ObjectId>,
        rule: gaanim_core::peniko::Fill,
        invert: bool,
    },
    /// Insert an explicit zero-duration interactive stop.
    Stop,
    /// Set an object visible (instant).
    Show(ObjectId),
    /// Set an object invisible (instant).
    Hide(ObjectId),
    /// Remove an object completely.
    Remove(ObjectId),
    /// Reparent a drawable into an existing group while retaining its local transform.
    AttachToGroup { group: ObjectId, child: ObjectId },
    /// Reparent and place a drawable at a coordinate-space local point.
    PlaceAtCoordinate {
        space: ObjectId,
        target: ObjectId,
        local: DVec3,
    },
    /// Adopt an existing drawable into the active segment.
    Reuse(ObjectId),
    /// Make an existing drawable global from the current cursor onward.
    Persist(ObjectId),
    /// Return a global drawable to the active segment.
    Release(ObjectId),
    /// Recompute a layout container and optionally animate every affected child
    /// into its new position.
    LayoutTransition {
        from_version: Option<u64>,
        to: LayoutTreeSnapshot,
        duration: Option<f64>,
        entering: Option<ObjectId>,
        leaving: Option<ObjectId>,
    },
    /// Resolve relational constraints against the current geometry. The
    /// expressions use stable canvas object IDs and are remapped on replay.
    LayoutConstraints {
        constraints: Vec<LayoutConstraint>,
        duration: Option<f64>,
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
        dissipating_time: Option<f64>,
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
    /// Follow any connector endpoint, including derived points and anchors.
    AttachEndpointFollow {
        target: ObjectId,
        endpoint: CanvasEndpoint,
        offset: DVec3,
        offset_space: FollowOffsetSpace,
    },
    /// Attach a TrackingLine — reactive line between two endpoints.
    AttachTrackingLine {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
    },
    /// Attach a live frame to compiled drawable or text-selection bounds.
    AttachSurroundingRect {
        target: ObjectId,
        sources: Vec<crate::anim::BoundsTarget>,
        padding: [f64; 4],
        corner_radius: f64,
    },
    /// Attach a helical spring whose endpoints follow entities or fixed positions.
    AttachTrackingSpring {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        coils: usize,
        amplitude: f64,
        crossing: f64,
        start_straight: f64,
        end_straight: f64,
    },
    /// Attach a dynamic dimension line between two endpoints.
    AttachTrackingDimension {
        line: ObjectId,
        extensions: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
        line_width: f64,
        extension_dash: Option<(f64, f64)>,
    },
    /// Drive a float signal from the current distance between two endpoints.
    AttachEndpointDistance {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        scale: f64,
    },
    /// Keep a dimension annotation on the displaced midpoint of two endpoints.
    AttachDimensionLabelPlacement {
        target: ObjectId,
        label: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        offset: f64,
        gap: f64,
        orientation: DimensionLabelOrientation,
    },
    /// Regenerate all visible parts of an angular dimension.
    AttachTrackingAngle {
        arc: ObjectId,
        arrows: ObjectId,
        extensions: ObjectId,
        vertex: CanvasEndpoint,
        from: CanvasRay,
        to: CanvasRay,
        radius: f64,
        sweep: AngleSweep,
        arrowheads: AngleArrowheads,
    },
    /// Drive a readout signal from a reactive angle.
    AttachEndpointAngle {
        target: ObjectId,
        vertex: CanvasEndpoint,
        from: CanvasRay,
        to: CanvasRay,
        sweep: AngleSweep,
        scale: f64,
    },
    /// Keep an angular annotation on the arc bisector.
    AttachAngleLabelPlacement {
        target: ObjectId,
        label: ObjectId,
        vertex: CanvasEndpoint,
        from: CanvasRay,
        to: CanvasRay,
        radius: f64,
        gap: f64,
        sweep: AngleSweep,
        orientation: DimensionLabelOrientation,
    },
    /// Regenerate a solid head for a reactive vector.
    AttachTrackingVectorHead {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
        length: f64,
        width: f64,
    },
    /// Couple a target's rotation to a source drawable.
    AttachRotationBinding {
        target: ObjectId,
        source: ObjectId,
        ratio: f64,
        phase: f64,
    },
    /// Convert a source rotation into a target translation.
    AttachRotationTranslationBinding {
        target: ObjectId,
        source: ObjectId,
        axis: DVec3,
        scale: f64,
    },
    /// Drive a property along a sampled `(times, values)` series as a pure
    /// function of timeline time (no per-frame callbacks).
    AttachSampledSeries {
        target: ObjectId,
        driver: SampledSeriesDriver,
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
    /// Custom callback updater retained by the deferred canvas operation.
    AttachCustomUpdater { target: ObjectId, updater: Updater },
    AttachReactiveArrowField2D {
        target: ObjectId,
        function: ReactiveFunction,
        position: [f64; 2],
        map: gaanim_visualization::CoordinateMap2D,
        options: super::ArrowFieldOptions,
        color_range: (f64, f64),
    },
    AttachReactiveArrowField3D {
        target: ObjectId,
        function: ReactiveFunction,
        resolution: [usize; 3],
        map: gaanim_visualization::CoordinateMap3D,
        options: super::ArrowFieldOptions,
        color_range: (f64, f64),
    },
    AttachReactiveStreamLine2D {
        target: ObjectId,
        function: ReactiveFunction,
        seed: [f64; 2],
        map: gaanim_visualization::CoordinateMap2D,
        style: super::StreamLinesStyle,
        color_range: (f64, f64),
    },
    AttachReactiveStreamLine3D {
        target: ObjectId,
        function: ReactiveFunction,
        seed: [f64; 3],
        map: gaanim_visualization::CoordinateMap3D,
        style: super::StreamLinesStyle,
        color_range: (f64, f64),
    },
    /// 3D traced path that accumulates source position as a LineList with optional colormap.
    AttachTracedPath3D {
        target: ObjectId,
        source: ObjectId,
        min_distance: f64,
        max_points: Option<usize>,
        colormap: Option<gaanim_core::ColorMap>,
        dissipating_time: Option<f64>,
    },
}

/// Visual preset used by [`Op::FragmentReveal`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FragmentRevealStyle {
    Fade,
    Wipe,
    FromBelow,
}

/// A tracking endpoint at the SceneModel level (before entity resolution).
#[derive(Debug, Clone)]
pub enum CanvasEndpoint {
    /// Fixed position in space.
    Static(DVec3),
    /// Position follows an entity (referenced by SceneModel ObjectId).
    Entity(ObjectId),
    /// A normalized anchor inside an entity's local bounds plus a local offset.
    Anchor(AnchorPoint),
    /// Point evaluated from two deterministic scalar sources.
    Expression { x: ScalarSource, y: ScalarSource },
    /// Scalar sources evaluated in an object's local coordinate frame.
    LocalExpression {
        space: ObjectId,
        x: ScalarSource,
        y: ScalarSource,
        z: ScalarSource,
    },
    /// Scalar data value mapped through a number line at runtime.
    LocalNumberLine {
        space: ObjectId,
        axis: gaanim_visualization::Axis,
        length: f64,
        value: ScalarSource,
        normal_offset: ScalarSource,
    },
    /// Reactive scene-space displacement from another endpoint.
    Offset {
        origin: Box<CanvasEndpoint>,
        dx: ScalarSource,
        dy: ScalarSource,
    },
    /// Affine interpolation between endpoints plus a scene-space offset.
    Between {
        from: Box<CanvasEndpoint>,
        to: Box<CanvasEndpoint>,
        alpha: f64,
        offset: DVec3,
    },
    /// Polar point around another endpoint.
    Polar {
        origin: Box<CanvasEndpoint>,
        radius: ScalarSource,
        angle: ScalarSource,
    },
}

/// Lightweight public reference to a derived, non-rendered endpoint.
#[derive(Debug, Clone)]
pub struct PointRef(pub CanvasEndpoint);

impl From<PointRef> for CanvasEndpoint {
    fn from(value: PointRef) -> Self {
        value.0
    }
}

/// A ray for angular and local-frame annotations.
#[derive(Debug, Clone)]
pub enum CanvasRay {
    Direction(DVec3),
    Endpoint(CanvasEndpoint),
}

/// A non-rendered reactive point attached to a drawable's local bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnchorPoint {
    pub object: ObjectId,
    pub normalized: DVec3,
    pub offset: DVec3,
}

impl From<AnchorPoint> for CanvasEndpoint {
    fn from(value: AnchorPoint) -> Self {
        Self::Anchor(value)
    }
}

/// Preset updater types that can be attached to entities via the SceneModel API.
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

/// Stop stored in authoring-local time until the segment manifest is compiled.
#[derive(Debug, Clone)]
pub(crate) struct LocalSegmentStop {
    pub name: Option<String>,
    pub time: f64,
}

/// A named segment (≈ scene) within a [`SceneModel`](super::SceneModel).
#[derive(Debug, Clone)]
pub struct Segment {
    pub id: SegmentId,
    pub name: String,
    pub notes: Option<String>,
    pub template: Option<String>,
    /// Optional full-canvas paint used while this segment is active.
    pub background: Option<BackgroundPaint>,
    pub(crate) stops: Vec<LocalSegmentStop>,
    pub explicit: bool,
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
    pub(crate) fn implicit() -> Self {
        Self {
            id: SegmentId(0),
            name: "_default".to_string(),
            notes: None,
            template: None,
            background: None,
            stops: Vec::new(),
            explicit: false,
            cursor: 0.0,
            ops: Vec::new(),
            transition: None,
            prev_segment: None,
            mobject_ids: Vec::new(),
        }
    }

    pub(crate) fn new(
        id: SegmentId,
        name: String,
        notes: Option<String>,
        template: Option<String>,
        background: Option<BackgroundPaint>,
    ) -> Self {
        Self {
            id,
            name,
            notes,
            template,
            background,
            stops: Vec::new(),
            explicit: true,
            cursor: 0.0,
            ops: Vec::new(),
            transition: None,
            prev_segment: None,
            mobject_ids: Vec::new(),
        }
    }

    pub(crate) fn is_untouched_implicit(&self) -> bool {
        !self.explicit
            && self.cursor.abs() <= f64::EPSILON
            && self.ops.is_empty()
            && self.mobject_ids.is_empty()
            && self.stops.is_empty()
    }
}
