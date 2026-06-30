//! Deferred operations and segment tracking for the Canvas API.

use std::sync::{Arc, Mutex};

use gaanim_core::ObjectId;
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
    /// Insert a slide breakpoint.
    Slide,
    /// Set an object visible (instant).
    Show(ObjectId),
    /// Set an object invisible (instant).
    Hide(ObjectId),
    /// Remove an object completely.
    Remove(ObjectId),
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
