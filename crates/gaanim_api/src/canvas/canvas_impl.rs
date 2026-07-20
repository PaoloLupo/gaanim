//! Canvas — the top-level facade for building Gaanim animations.

use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use gaanim_core::peniko::Color;
use gaanim_timeline::transition::TransitionType;

use crate::anim::{AnimationBuilder, AnimationType};
use crate::canvas::drawable::DrawableHandle;
use crate::canvas::ops::{CanvasEndpoint, CanvasState, Op, Segment, SharedCanvasState};
use crate::canvas::types::{Anim, CoordinateSystem, Margin, SpawnKind};

/// Top-level facade for building Gaanim animations.
#[derive(Debug, Clone)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub background: Option<Color>,
    pub units: CoordinateSystem,
    pub theme: Option<String>,
    pub margin: Margin,
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
            margin: Margin::default(),
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
        self.play_with_lag(anims, 0.0);
    }

    /// Parallel playback with a uniform stagger between each animation's
    /// start time. Existing per-animation delays are preserved and the lag is
    /// added on top.
    pub fn play_with_lag(&mut self, anims: Vec<Anim>, lag: f64) {
        let lag = lag.max(0.0);
        let builders: Vec<AnimationBuilder> = anims
            .into_iter()
            .enumerate()
            .map(|(idx, anim)| {
                anim.deactivate_auto_queue();
                let mut anim = anim.into_builder();
                anim.delay += idx as f64 * lag;
                anim
            })
            .collect();
        self.play_builders(builders);
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

    pub fn reuse(&mut self, o: &DrawableHandle) {
        self.state
            .lock()
            .expect("canvas state poisoned")
            .active_mut()
            .mobject_ids
            .push(o.id);
    }

    // -- Reactive objects --

    /// Spawn a value tracker — a reactive float signal that can be animated
    /// with `.animate_to()` and referenced by other reactive components.
    pub fn value_tracker(&mut self, initial: f64) -> DrawableHandle {
        self.spawn(SpawnKind::ValueTracker(initial))
    }

    /// Spawn a traced path that accumulates the trajectory of `source` as a
    /// continuous line. The returned drawable's Path2D is regenerated every frame.
    pub fn traced_path(&mut self, source: &DrawableHandle) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TracedPathLine);
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
                min_distance: 1.0,
                max_points: None,
            });
        handle
    }

    /// Spawn a tracking line — a reactive line whose endpoints follow entities
    /// or remain at fixed positions. Updated every frame.
    ///
    /// Endpoints can be `DrawableHandle` references (their `.id` is used) or
    /// static `(f64, f64)` positions passed as tuples.
    pub fn tracking_line(&mut self, from: CanvasEndpoint, to: CanvasEndpoint) -> DrawableHandle {
        let handle = self.spawn(SpawnKind::TrackingLine);
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

    // -- Render / export --

    pub fn render(&self) -> bool {
        crate::host::send_to_host(self.clone())
    }

    pub fn export(
        &self,
        path: &str,
        fps: Option<u32>,
        _encoder: Option<&str>,
        transparent: Option<bool>,
    ) -> Result<(), gaanim_export::encoder::ExportError> {
        crate::export::export_canvas_to_path(self.clone(), path, fps, transparent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::ops::Op;
    use bevy::prelude::World;
    use gaanim_scene::MobjectId;
    use gaanim_timeline::scene::SceneMember;
    use gaanim_timeline::timeline::Timeline;

    #[test]
    fn play_with_lag_offsets_delays_and_cursor() {
        let mut canvas = Canvas::new(1280, 720);
        let first = canvas.circle(20.0);
        let second = canvas.circle(20.0);

        canvas.play_with_lag(vec![first.fade_in(1.0), second.fade_in(1.0)], 0.25);

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
    fn play_builders_counts_existing_delays_in_cursor() {
        let mut canvas = Canvas::new(1280, 720);
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
    fn cross_segment_transform_does_not_retag_source_in_initial_snapshot() {
        let mut canvas = Canvas::new(1280, 720);
        canvas.segment("first", None);
        let circle = canvas.circle(40.0);
        let diamond = canvas.rect(80.0, 80.0);
        circle.transform(&diamond).duration(1.0);

        canvas.segment(
            "second",
            Some(gaanim_timeline::transition::TransitionType::CrossFade { duration: 0.2 }),
        );
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
}
