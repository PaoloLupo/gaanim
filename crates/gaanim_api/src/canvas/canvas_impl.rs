//! Canvas — the top-level facade for building Gaanim animations.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use gaanim_core::peniko::Color;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{CanvasState, Op, Segment, SharedCanvasState};
use crate::canvas::types::{Anim, CoordinateSystem, SpawnKind};

/// Top-level facade for building Gaanim animations.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub background: Option<Color>,
    pub units: CoordinateSystem,
    pub theme: Option<String>,
    pub(crate) state: SharedCanvasState,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            background: None,
            theme: None,
            units: CoordinateSystem::Pixels,
            state: Arc::new(Mutex::new(CanvasState::new())),
        }
    }

    pub fn background(mut self, c: Color) -> Self {
        self.background = Some(c);
        self
    }

    pub fn with_units(mut self, u: CoordinateSystem) -> Self {
        self.units = u;
        self
    }

    pub fn current_time(&self) -> f64 {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active()
            .cursor
    }

    pub fn segment_count(&self) -> usize {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .segments
            .len()
    }

    fn spawn(&mut self, kind: SpawnKind) -> DrawableHandle {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let id = guard.next_object_id();
        let active_idx = guard.active_idx;
        guard.active_mut().mobject_ids.push(id);
        guard.all_drawables.push(id);
        drop(guard);

        let handle = DrawableHandle::new(id, kind, self.state.clone(), active_idx);
        self.state.lock().expect("canvas state poisoned").segments[active_idx]
            .ops
            .push(Op::Spawn(handle.spec.clone()));
        handle
    }

    // -- Segment management --

    /// Create a named segment and switch to it. If `transition` is `Some`, it is
    /// linked from the previously active segment.
    pub fn segment(&mut self, name: &str, transition: Option<TransitionType>) -> usize {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        let prev = guard.active_idx;
        let mut seg = Segment::new(name);
        seg.transition = transition;
        seg.prev_segment = Some(prev);
        let idx = guard.segments.len();
        guard.segments.push(seg);
        guard.active_idx = idx;
        idx
    }

    /// Explicitly link two segments by index.
    pub fn link(&mut self, from: usize, to: usize, transition: TransitionType) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        if from < guard.segments.len() && to < guard.segments.len() {
            guard.segments[to].transition = Some(transition);
            guard.segments[to].prev_segment = Some(from);
        }
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
    pub fn arrow(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> DrawableHandle {
        self.spawn(SpawnKind::Arrow(x1, y1, x2, y2))
    }
    pub fn text(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Text(s.to_string()))
    }
    pub fn title(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Title(s.to_string()))
    }
    pub fn subtitle(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Subtitle(s.to_string()))
    }
    pub fn equation(&mut self, s: &str) -> DrawableHandle {
        self.spawn(SpawnKind::Equation(s.to_string()))
    }
    pub fn group(&mut self, members: &[&DrawableHandle]) -> DrawableHandle {
        self.spawn(SpawnKind::Group(members.iter().map(|m| m.id).collect()))
    }

    // -- Time controls --

    pub fn wait(&mut self, dur: f64) {
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += dur.max(0.0);
        guard.active_mut().ops.push(Op::Wait(dur.max(0.0)));
    }

    /// Regroup auto-queued animations into a parallel batch at the current
    /// cursor. Each `Anim` passed here is deactivated from its original
    /// sequential position before the batch is inserted.
    pub fn play(&mut self, anims: Vec<Anim>) {
        let builders: Vec<AnimationBuilder> = anims
            .into_iter()
            .map(|anim| {
                anim.deactivate_auto_queue();
                anim.into_builder()
            })
            .collect();
        self.play_builders(builders);
    }

    /// Low-level parallel playback for legacy `AnimationBuilder` values.
    pub fn play_builders(&mut self, anims: Vec<AnimationBuilder>) {
        let max_dur = anims.iter().map(|a| a.duration).fold(0.0, f64::max);
        let mut guard = self.state.lock().expect("canvas state poisoned");
        guard.active_mut().cursor += max_dur;
        guard.active_mut().ops.push(Op::Play(anims));
    }

    pub fn slide(&mut self) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .ops
            .push(Op::Slide);
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

    pub fn reuse(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .mobject_ids
            .push(o.id);
    }

    // -- Render / export --

    pub fn render(&self) -> bool {
        crate::host::send_to_host(self.clone())
    }

    pub fn export(&self, path: &str, fps: Option<u32>, _enc: Option<&str>, _trans: Option<bool>) {
        info!(
            "Canvas export({}): {}x{} @{}fps",
            path,
            self.width,
            self.height,
            fps.unwrap_or(60)
        );
    }
}
