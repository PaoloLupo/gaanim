//! Opt-in frame profiler for interactive playback.
//!
//! Set `GAANIM_FRAME_PROFILE=1` before launching the editor. Once the scene is
//! loaded, the timeline plays continuously from t=0 (authored stops are
//! ignored) and one line per wall-clock second is written to stderr, splitting
//! the frame into main-world phases (timeline seek, fragment compilation) and
//! render-world phases (surface acquire, render graph and present), with the
//! largest Vello scene of the second (paths, segments and clips). The editor
//! exits with a summary of the slowest windows when the timeline ends, and
//! one `GAANIM_PLAYBACK_TIMINGS` line of the whole playback for
//! `tests/benchmark_runtime.py --scenarios playback`.

use crate::EditorState;
use bevy::platform::time::Instant;
use bevy::prelude::*;
use bevy::render::renderer::render_system;
use bevy::render::view::window::prepare_windows;
use bevy::render::{Render, RenderApp, RenderSystems};
use gaanim_core::ObjectId;
use gaanim_renderer::pipeline::{GaanimRenderCache, gaanim_render_system};
use gaanim_renderer::prelude::{PreviewResolution, VelloDiagnostics};
use gaanim_timeline::timeline::Timeline;
use gaanim_timeline::{timeline_playback_system, timeline_seek_system};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

/// Frames rendered after the scene loads, before playback starts.
const WARMUP_FRAMES: u32 = 30;

pub fn enabled() -> bool {
    std::env::var_os("GAANIM_FRAME_PROFILE").is_some_and(|value| value != "0")
}

#[derive(Clone, Copy)]
struct Phase {
    total: Duration,
    max: Duration,
}

impl Phase {
    const ZERO: Self = Self {
        total: Duration::ZERO,
        max: Duration::ZERO,
    };

    fn add(&mut self, elapsed: Duration) {
        self.total += elapsed;
        self.max = self.max.max(elapsed);
    }

    fn avg_ms(&self, frames: u32) -> f64 {
        self.total.as_secs_f64() * 1000.0 / f64::from(frames.max(1))
    }

    fn max_ms(&self) -> f64 {
        self.max.as_secs_f64() * 1000.0
    }
}

struct RenderWorldTimes {
    frames: u32,
    frame: Phase,
    acquire: Phase,
    graph: Phase,
}

impl RenderWorldTimes {
    const EMPTY: Self = Self {
        frames: 0,
        frame: Phase::ZERO,
        acquire: Phase::ZERO,
        graph: Phase::ZERO,
    };
}

/// Render-world timings, drained by the main-world reporter once per window.
static RENDER_TIMES: Mutex<RenderWorldTimes> = Mutex::new(RenderWorldTimes::EMPTY);

#[derive(Default)]
struct RenderMarks {
    frame: Option<Instant>,
    acquire: Option<Instant>,
    graph: Option<Instant>,
}

#[derive(Default, PartialEq, Eq)]
enum ProfileState {
    #[default]
    WaitingForScene,
    WarmingUp(u32),
    Playing,
    Done,
}

struct Window {
    started: Instant,
    frames: u32,
    frame_dt: Phase,
    main: Phase,
    seek: Phase,
    compile: Phase,
    rebuilt: u64,
    fragments: usize,
    /// Visible and total Mobjects at the end of the window.
    visible: usize,
    mobjects: usize,
    /// Lowest preview resolution scale used in the window.
    preview_scale: f32,
    /// Largest Vello scene of the window: paths, path segments and clips.
    vello: [u32; 3],
    timeline_start: f64,
    timeline_end: f64,
    segment: Option<String>,
}

impl Window {
    fn new(timeline_time: f64) -> Self {
        Self {
            started: Instant::now(),
            frames: 0,
            frame_dt: Phase::ZERO,
            main: Phase::ZERO,
            seek: Phase::ZERO,
            compile: Phase::ZERO,
            rebuilt: 0,
            fragments: 0,
            vello: [0; 3],
            visible: 0,
            mobjects: 0,
            preview_scale: 1.0,
            timeline_start: timeline_time,
            timeline_end: timeline_time,
            segment: None,
        }
    }
}

struct WindowReport {
    line: String,
    avg_dt_ms: f64,
}

/// Frame times and phase totals of the whole playback.
#[derive(Default)]
struct Playback {
    started: Option<Instant>,
    /// Milliseconds between consecutive frames, and of each main-world frame.
    frame_ms: Vec<f64>,
    main_ms: Vec<f64>,
    seek: Duration,
    compile: Duration,
    render: Duration,
    render_frames: u32,
}

impl Playback {
    fn summary(&self) -> String {
        let frames = self.main_ms.len();
        let per_frame =
            |total: Duration, frames: usize| total.as_secs_f64() * 1000.0 / frames.max(1) as f64;
        format!(
            "GAANIM_PLAYBACK_TIMINGS frames={frames} seconds={:.3} frame_p50_ms={:.3} frame_p95_ms={:.3} main_p50_ms={:.3} main_p95_ms={:.3} seek_ms={:.3} compile_ms={:.3} render_ms={:.3}",
            self.started
                .map_or(0.0, |started| started.elapsed().as_secs_f64()),
            percentile(&self.frame_ms, 0.50),
            percentile(&self.frame_ms, 0.95),
            percentile(&self.main_ms, 0.50),
            percentile(&self.main_ms, 0.95),
            per_frame(self.seek, frames),
            per_frame(self.compile, frames),
            per_frame(self.render, self.render_frames as usize),
        )
    }
}

/// The value below which `fraction` of `values` lie (nearest rank).
fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = (fraction * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[derive(Resource, Default)]
struct FrameProfile {
    state: ProfileState,
    window: Option<Window>,
    reports: Vec<WindowReport>,
    playback: Playback,
    main_start: Option<Instant>,
    seek_start: Option<Instant>,
    compile_start: Option<Instant>,
    last_frame: Option<Instant>,
    fragment_ptrs: HashMap<ObjectId, usize>,
}

pub struct FrameProfilePlugin;

impl Plugin for FrameProfilePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameProfile>()
            .add_systems(First, main_frame_start)
            .add_systems(
                Update,
                (
                    drive_playback.before(timeline_playback_system),
                    seek_start.before(timeline_seek_system),
                    seek_end.after(timeline_seek_system),
                    compile_start.before(gaanim_render_system),
                    compile_end.after(gaanim_render_system),
                ),
            )
            .add_systems(Last, main_frame_end);

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };
        render_app.add_systems(
            Render,
            (
                render_frame_start.in_set(RenderSystems::ExtractCommands),
                acquire_start
                    .in_set(RenderSystems::PrepareViews)
                    .before(prepare_windows),
                acquire_end
                    .in_set(RenderSystems::PrepareViews)
                    .after(prepare_windows),
                graph_start
                    .in_set(RenderSystems::Render)
                    .before(render_system),
                graph_end.in_set(RenderSystems::Render).after(render_system),
                render_frame_end.in_set(RenderSystems::PostCleanup),
            ),
        );
    }
}

fn main_frame_start(mut profile: ResMut<FrameProfile>) {
    profile.main_start = Some(Instant::now());
}

fn drive_playback(
    mut profile: ResMut<FrameProfile>,
    mut timeline: ResMut<Timeline>,
    mut editor_state: ResMut<EditorState>,
    mut exit: MessageWriter<AppExit>,
) {
    match profile.state {
        ProfileState::WaitingForScene if timeline.cached_duration > 0.0 => {
            profile.state = ProfileState::WarmingUp(0);
        }
        ProfileState::WarmingUp(frames) if frames >= WARMUP_FRAMES => {
            editor_state.continuous_preview = true;
            timeline.loop_range = None;
            timeline.seek_request = Some(0.0);
            timeline.is_playing = true;
            profile.state = ProfileState::Playing;
            profile.window = Some(Window::new(0.0));
            profile.playback = Playback {
                started: Some(Instant::now()),
                ..default()
            };
            profile.last_frame = None;
            eprintln!(
                "GAANIM_FRAME_PROFILE playing {:.2}s timeline (all ms are per-frame averages; max in brackets)",
                timeline.cached_duration
            );
        }
        ProfileState::WarmingUp(frames) => profile.state = ProfileState::WarmingUp(frames + 1),
        ProfileState::Playing
            if !timeline.is_playing && timeline.current_time >= timeline.cached_duration - 1e-6 =>
        {
            flush_window(&mut profile);
            print_summary(&profile.reports);
            eprintln!("{}", profile.playback.summary());
            profile.state = ProfileState::Done;
            exit.write(AppExit::Success);
        }
        _ => {}
    }
}

fn seek_start(mut profile: ResMut<FrameProfile>) {
    profile.seek_start = Some(Instant::now());
}

fn seek_end(mut profile: ResMut<FrameProfile>) {
    let profile = &mut *profile;
    let elapsed = profile.seek_start.take().map(|start| start.elapsed());
    if let (Some(window), Some(elapsed)) = (profile.window.as_mut(), elapsed) {
        window.seek.add(elapsed);
        profile.playback.seek += elapsed;
    }
}

fn compile_start(mut profile: ResMut<FrameProfile>) {
    profile.compile_start = Some(Instant::now());
}

fn compile_end(mut profile: ResMut<FrameProfile>, cache: Option<Res<GaanimRenderCache>>) {
    let elapsed = profile.compile_start.take().map(|start| start.elapsed());
    let Some(cache) = cache else { return };
    let profile = &mut *profile;
    let mut rebuilt = 0;
    for (id, fragment) in &cache.fragment_cache {
        let ptr = std::sync::Arc::as_ptr(fragment) as usize;
        if profile.fragment_ptrs.insert(*id, ptr) != Some(ptr) {
            rebuilt += 1;
        }
    }
    if let (Some(window), Some(elapsed)) = (profile.window.as_mut(), elapsed) {
        profile.playback.compile += elapsed;
        window.compile.add(elapsed);
        window.rebuilt += rebuilt;
        window.fragments = cache.fragment_cache.len();
    }
}

fn main_frame_end(
    mut profile: ResMut<FrameProfile>,
    timeline: Option<Res<Timeline>>,
    preview: Option<Res<PreviewResolution>>,
    vello: Option<Res<VelloDiagnostics>>,
    mobjects: Query<Has<gaanim_scene::Visible>, With<gaanim_scene::MobjectId>>,
) {
    let now = Instant::now();
    let main = profile.main_start.take().map(|start| now - start);
    let dt = profile.last_frame.replace(now).map(|last| now - last);
    if profile.state != ProfileState::Playing {
        return;
    }
    let playback = &mut profile.playback;
    if let Some(main) = main {
        playback.main_ms.push(main.as_secs_f64() * 1000.0);
    }
    if let Some(dt) = dt {
        playback.frame_ms.push(dt.as_secs_f64() * 1000.0);
    }
    let Some(window) = profile.window.as_mut() else {
        return;
    };
    window.frames += 1;
    if let Some(preview) = preview {
        window.preview_scale = window.preview_scale.min(preview.scale);
    }
    if let Some(vello) = vello {
        let counts = [vello.paths, vello.path_segments, vello.clips];
        for (largest, count) in window.vello.iter_mut().zip(counts) {
            *largest = (*largest).max(count.unwrap_or(0));
        }
    }
    if let Some(main) = main {
        window.main.add(main);
    }
    if let Some(dt) = dt {
        window.frame_dt.add(dt);
    }
    if let Some(timeline) = timeline {
        window.timeline_end = timeline.current_time;
        if window.segment.is_none() {
            window.segment = timeline.segment_label();
        }
    }
    if window.started.elapsed() >= Duration::from_secs(1) {
        window.mobjects = 0;
        window.visible = 0;
        for visible in &mobjects {
            window.mobjects += 1;
            window.visible += usize::from(visible);
        }
        let next = window.timeline_end;
        flush_window(&mut profile);
        profile.window = Some(Window::new(next));
    }
}

fn flush_window(profile: &mut FrameProfile) {
    let Some(window) = profile.window.take() else {
        return;
    };
    let render = std::mem::replace(
        &mut *RENDER_TIMES.lock().expect("frame profile poisoned"),
        RenderWorldTimes::EMPTY,
    );
    profile.playback.render += render.frame.total;
    profile.playback.render_frames += render.frames;
    if window.frames == 0 {
        return;
    }
    let frames = window.frames;
    let avg_dt_ms = window.frame_dt.avg_ms(frames);
    let line = format!(
        "t={:6.2}-{:6.2}s {:>3} fps | frame {:5.1} [{:5.1}] | main {:5.1} [{:5.1}] seek {:5.1} [{:5.1}] compile {:5.1} [{:5.1}] rebuilt {:5.1}/{:<5} | render {:5.1} [{:5.1}] acquire {:5.1} [{:5.1}] graph+present {:5.1} [{:5.1}] preview {:3.0}% | paths {} segs {} clips {} | visible {}/{} | {}",
        window.timeline_start,
        window.timeline_end,
        (f64::from(frames) / window.started.elapsed().as_secs_f64()).round(),
        avg_dt_ms,
        window.frame_dt.max_ms(),
        window.main.avg_ms(frames),
        window.main.max_ms(),
        window.seek.avg_ms(frames),
        window.seek.max_ms(),
        window.compile.avg_ms(frames),
        window.compile.max_ms(),
        window.rebuilt as f64 / f64::from(frames),
        window.fragments,
        render.frame.avg_ms(render.frames),
        render.frame.max_ms(),
        render.acquire.avg_ms(render.frames),
        render.acquire.max_ms(),
        render.graph.avg_ms(render.frames),
        render.graph.max_ms(),
        window.preview_scale * 100.0,
        window.vello[0],
        window.vello[1],
        window.vello[2],
        window.visible,
        window.mobjects,
        window.segment.as_deref().unwrap_or("-"),
    );
    eprintln!("GAANIM_FRAME_PROFILE {line}");
    profile.reports.push(WindowReport { line, avg_dt_ms });
}

fn print_summary(reports: &[WindowReport]) {
    let mut slowest: Vec<_> = reports.iter().collect();
    slowest.sort_by(|a, b| b.avg_dt_ms.total_cmp(&a.avg_dt_ms));
    eprintln!("GAANIM_FRAME_PROFILE slowest windows:");
    for report in slowest.iter().take(5) {
        eprintln!("GAANIM_FRAME_PROFILE   {}", report.line);
    }
}

/// Phase start marks of the render frame in flight.
static RENDER_MARKS: Mutex<RenderMarks> = Mutex::new(RenderMarks {
    frame: None,
    acquire: None,
    graph: None,
});

fn render_frame_start() {
    *RENDER_MARKS.lock().expect("frame profile poisoned") = RenderMarks {
        frame: Some(Instant::now()),
        ..default()
    };
}

fn acquire_start() {
    RENDER_MARKS.lock().expect("frame profile poisoned").acquire = Some(Instant::now());
}

fn acquire_end() {
    let start = RENDER_MARKS
        .lock()
        .expect("frame profile poisoned")
        .acquire
        .take();
    if let Some(start) = start {
        RENDER_TIMES
            .lock()
            .expect("frame profile poisoned")
            .acquire
            .add(start.elapsed());
    }
}

fn graph_start() {
    RENDER_MARKS.lock().expect("frame profile poisoned").graph = Some(Instant::now());
}

fn graph_end() {
    let start = RENDER_MARKS
        .lock()
        .expect("frame profile poisoned")
        .graph
        .take();
    if let Some(start) = start {
        RENDER_TIMES
            .lock()
            .expect("frame profile poisoned")
            .graph
            .add(start.elapsed());
    }
}

fn render_frame_end() {
    let start = RENDER_MARKS
        .lock()
        .expect("frame profile poisoned")
        .frame
        .take();
    if let Some(start) = start {
        let mut times = RENDER_TIMES.lock().expect("frame profile poisoned");
        times.frames += 1;
        times.frame.add(start.elapsed());
    }
}
