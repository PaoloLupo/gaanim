//! Deferred operations and segment tracking for the Canvas API.

use std::sync::{Arc, Mutex};

use gaanim_animation::AxisMask;
use gaanim_core::ObjectId;
use gaanim_core::glam::{DQuat, DVec3};
use gaanim_timeline::transition::TransitionType;

use crate::anim::AnimationBuilder;
use crate::canvas::types::ObjectSpec;

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
    /// Insert a slide breakpoint.
    Slide,
    /// Set an object visible (instant).
    Show(ObjectId),
    /// Set an object invisible (instant).
    Hide(ObjectId),
    /// Remove an object completely.
    Remove(ObjectId),

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
    /// Attach a TrackingLine — reactive line between two endpoints.
    AttachTrackingLine {
        target: ObjectId,
        from: CanvasEndpoint,
        to: CanvasEndpoint,
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
