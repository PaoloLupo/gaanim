//! Record a scene into a playback bundle (see [`gaanim_bundle`]).
//!
//! The recording steps the world exactly like [`crate::exporter::export_scene_direct`]:
//! one timeline seek and one update per frame on a fixed grid, so updaters,
//! reactive bindings and every Python callback run as they do for a video.
//! Each frame then records what the 2D renderer draws instead of pixels.

use std::path::{Path, PathBuf};
use std::time::Instant;

use bevy::prelude::*;
use gaanim_bundle::{AudioData, BundleWriter, Frame, PostPass, SceneData, SceneSpan};
use gaanim_core::console;
use gaanim_renderer::pipeline::{CanvasBackground, capture_frame, capture_frame_pinned};
use gaanim_renderer::post_process::{CanvasPostProcess, PostProcessShader};
use gaanim_timeline::timeline::Timeline;

use crate::config::ExportTelemetry;
use crate::encoder::{ExportError, Result};
use crate::exporter::{check_custom_animation_errors, frame_camera};

/// Settings of a bundle recording.
#[derive(Clone, Debug)]
pub struct BundleConfig {
    pub output_path: PathBuf,
    /// Name shown for the bundle.
    pub title: String,
    /// Frames per second of the recording grid.
    pub fps: u32,
    /// Pixel size the scene is authored for; it sets antialiasing margins
    /// and the resolution of shader backgrounds.
    pub width: u32,
    pub height: u32,
    pub telemetry: Option<ExportTelemetry>,
    /// Record the instants between grid frames in a second world even when
    /// the scene keeps no state that depends on the instants it visited.
    /// Both recordings are identical; this exists to check that.
    pub force_second_world: bool,
    /// Timeline instant of the cover image (`scene.thumbnail(t)`); `None`
    /// picks the first stop, or the fullest frame of the first segment.
    pub thumbnail_time: Option<f64>,
    /// Render and store the cover image. Without a GPU the bundle is still
    /// recorded, without one.
    pub thumbnail: bool,
}

impl BundleConfig {
    pub fn new(output_path: impl Into<PathBuf>) -> Self {
        let output_path = output_path.into();
        let title = output_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Gaanim")
            .to_owned();
        Self {
            output_path,
            title,
            fps: 60,
            width: 1920,
            height: 1080,
            telemetry: None,
            force_second_world: false,
            thumbnail_time: None,
            thumbnail: true,
        }
    }
}

/// Instants a bundle records.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordingPlan {
    /// The frame grid of an export at the recording rate, stepped exactly as
    /// an export steps it, so a video exported from the bundle renders the
    /// very frames it would have rendered from the scene.
    pub grid: Vec<f64>,
    /// Instants off the grid that playback rests on or jumps to (the end,
    /// segment boundaries, stops, ambient loop ends and markers), so a
    /// presentation at rest shows exactly the frame the editor shows.
    pub extras: Vec<f64>,
}

impl RecordingPlan {
    pub fn new(timeline: &Timeline, fps: u32) -> Self {
        let duration = timeline.cached_duration.max(0.0);
        let fps = fps.max(1);
        let step = 1.0 / f64::from(fps);
        let frames = (duration * f64::from(fps)).ceil() as u64;
        let mut grid = Vec::with_capacity(frames as usize);
        let mut time = 0.0;
        for _ in 0..frames {
            grid.push(time);
            time += step;
        }
        let mut extras = vec![duration];
        for segment in &timeline.segments {
            extras.push(segment.start_time);
            extras.push(segment.end_time);
            for stop in &segment.stops {
                extras.push(stop.time);
                if let Some(ambient) = stop.ambient {
                    extras.push(stop.time + ambient);
                }
            }
        }
        extras.extend(timeline.markers.iter().map(|marker| marker.time));
        extras.retain(|time| time.is_finite() && (0.0..=duration).contains(time));
        extras.sort_by(f64::total_cmp);
        extras.dedup_by(|a, b| (*a - *b).abs() <= 1e-9);
        // The grid already shows an instant this close to one of its frames.
        extras.retain(|time| {
            let next = grid.partition_point(|frame| frame < time);
            let near = |index: usize| {
                grid.get(index)
                    .is_some_and(|frame| (frame - time).abs() <= 1e-9)
            };
            !near(next) && !(next > 0 && near(next - 1))
        });
        Self { grid, extras }
    }

    /// Grid instants the second world steps through before its last extra.
    pub fn second_pass_steps(&self) -> usize {
        self.extras
            .last()
            .map_or(0, |last| self.grid.partition_point(|time| time < last))
    }

    /// Every recorded instant, in time order.
    pub fn times(&self) -> Vec<f64> {
        let mut times: Vec<f64> = self.grid.iter().chain(&self.extras).copied().collect();
        times.sort_by(f64::total_cmp);
        times
    }
}

/// Every instant a bundle records at `fps`, in time order; see
/// [`RecordingPlan`].
pub fn recording_times(timeline: &Timeline, fps: u32) -> Vec<f64> {
    RecordingPlan::new(timeline, fps).times()
}

/// Terminal bar, editor telemetry and export-worker marker for a recording.
struct RecordingProgress {
    bar: indicatif::ProgressBar,
    telemetry: Option<ExportTelemetry>,
    total: u64,
    done: std::cell::Cell<u64>,
    window: std::cell::Cell<Instant>,
}

impl RecordingProgress {
    fn new(total: u64, telemetry: Option<ExportTelemetry>) -> Self {
        let bar = crate::exporter::create_progress_bar(total);
        bar.set_prefix("record");
        Self {
            bar,
            telemetry,
            total,
            done: std::cell::Cell::new(0),
            window: std::cell::Cell::new(Instant::now()),
        }
    }

    fn advance(&self) {
        let done = self.done.get() + 1;
        self.done.set(done);
        self.bar.inc(1);
        if done.is_multiple_of(30) || done == self.total {
            let elapsed = self.window.replace(Instant::now()).elapsed();
            self.bar
                .set_message(format!("{:.1} fps", 30.0 / elapsed.as_secs_f64().max(1e-9)));
            crate::exporter::export_progress(&self.telemetry, done, self.total);
        }
    }

    fn finish(&self) {
        self.bar.finish_and_clear();
    }
}

fn scene_spans(timeline: &Timeline) -> Vec<SceneSpan> {
    timeline
        .scene_index
        .values()
        .filter_map(|id| {
            let (start, end) = timeline.scene_bounds(*id)?;
            Some(SceneSpan {
                name: timeline.scenes.get(*id)?.name.clone(),
                start,
                end,
            })
        })
        .collect()
}

/// Shaders of every post-process chain, deduplicated.
fn post_shader_table(post: Option<&CanvasPostProcess>) -> Vec<PostProcessShader> {
    let mut shaders: Vec<PostProcessShader> = Vec::new();
    let Some(post) = post else {
        return shaders;
    };
    let chains =
        std::iter::once(post.passes.as_slice()).chain(post.segments.iter().filter_map(|segment| {
            match &segment.post {
                gaanim_renderer::post_process::PostProcessOverride::Passes(passes) => {
                    Some(passes.as_slice())
                }
                _ => None,
            }
        }));
    for chain in chains {
        for pass in chain {
            if !shaders.contains(&pass.shader) {
                shaders.push(pass.shader.clone());
            }
        }
    }
    shaders
}

/// Evaluate the post-process chain active at `time`, reading parameter
/// uniforms from the world.
fn post_passes(world: &World, shaders: &[PostProcessShader], time: f64) -> Result<Vec<PostPass>> {
    let Some(post) = world.get_resource::<CanvasPostProcess>() else {
        return Ok(Vec::new());
    };
    // The frame rectangle only matters to the GPU pass; any valid one
    // evaluates the same uniforms.
    let Some(request) = post.request_with(
        time,
        gaanim_core::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
        |entity| {
            world
                .get::<gaanim_animation::FloatSignal>(entity)
                .map(|signal| signal.value)
        },
    ) else {
        return Ok(Vec::new());
    };
    request
        .passes
        .into_iter()
        .map(|(shader, values)| {
            let index = shaders
                .iter()
                .position(|known| *known == shader)
                .ok_or_else(|| ExportError::Capture("unknown post-process shader".into()))?;
            Ok(PostPass {
                shader: index as u32,
                values,
            })
        })
        .collect()
}

/// Features the bundle format does not record yet; recording refuses them
/// instead of writing a bundle that plays back differently.
fn unsupported_content(world: &mut World) -> Option<&'static str> {
    if world
        .query_filtered::<(), With<gaanim_media::VideoPlayback>>()
        .iter(world)
        .next()
        .is_some()
    {
        return Some("video");
    }
    None
}

fn audio_data(
    writer: &mut BundleWriter<std::io::BufWriter<std::fs::File>>,
    world: &World,
) -> Result<Vec<AudioData>> {
    let Some(tracks) = world.get_resource::<gaanim_media::PreviewAudioTracks>() else {
        return Ok(Vec::new());
    };
    tracks
        .0
        .iter()
        .map(|track| {
            let media = writer.add_media(&track.path).map_err(|error| {
                ExportError::Capture(format!(
                    "could not embed audio '{}': {error}",
                    track.path.display()
                ))
            })?;
            Ok(AudioData {
                media,
                start_time: track.start_time,
                duration: track.duration,
                volume: track.volume,
                fade_in: track.fade_in,
                fade_out: track.fade_out,
                source_offset: track.source_offset,
                source_duration: track.source_duration,
                speed: track.speed,
                looping: track.looping,
            })
        })
        .collect()
}

/// The headless world a recording steps: the plugins of a direct export,
/// with the scene built and its first update run.
pub fn recording_app<F>(setup_world_fn: F) -> Result<App>
where
    F: FnOnce(&mut World) + Send + Sync + 'static,
{
    let mut app = App::new();
    app.add_plugins(bevy::prelude::MinimalPlugins)
        .add_plugins(gaanim_scene::GaanimScenePlugin)
        .add_plugins(gaanim_animation::GaanimAnimationPlugin)
        .add_plugins(gaanim_timeline::GaanimTimelinePlugin)
        .add_plugins(gaanim_media::GaanimMediaPlugin)
        .add_plugins(gaanim_text::GaanimTextPlugin)
        .add_plugins(gaanim_renderer::GaanimDerivedGeometryPlugin);
    app.insert_resource(crate::exporter::SetupCallback::new(setup_world_fn));
    app.add_systems(Startup, crate::exporter::setup_scene_system);
    app.finish();
    app.cleanup();
    app.update();
    check_custom_animation_errors(app.world())?;
    Ok(app)
}

/// Whether no state of the scene depends on the instants its timeline visited
/// before the current one, so visiting an extra instant between two grid
/// frames leaves the following grid frames unchanged.
///
/// Updaters advance by the seek deltas, traced paths and sampled series
/// accumulate, echoes and squash read earlier frames, and custom animations
/// and signal bindings run user code that may keep state; any of them keeps
/// the second world. Reactive callables (value trackers, property bindings,
/// redraw functions) must already be pure functions of their declared inputs
/// and time, since the editor seeks anywhere, and built-in lenses that report
/// [`history_free`](gaanim_animation::AnimatableLens::history_free) are pure.
fn history_free(world: &mut World) -> bool {
    use gaanim_animation as anim;
    use gaanim_timeline::clip::{ClipPayload, PropertyLensSpec};
    let mut stateful = world.query_filtered::<(), Or<(
        With<anim::Updater>,
        With<anim::SampledSeriesDrivers>,
        With<anim::TracedPath>,
        With<anim::TracedPath3D>,
        With<anim::SurroundingRect>,
        With<anim::SquashStretch>,
        With<anim::EchoGhost>,
        With<anim::SignalBinding>,
    )>>();
    if stateful.iter(world).next().is_some() {
        return false;
    }
    !world.resource::<Timeline>().clips.values().any(|clip| {
        matches!(
            &clip.payload,
            ClipPayload::Animation(animation)
                if matches!(&animation.lens, PropertyLensSpec::Dynamic(lens) if !lens.0.history_free())
        )
    })
}

/// Seek `app` to `time`, update it, and capture what it draws. A motion
/// blurred frame also steps and captures the sub-frames an export averages,
/// in the order an export seeks them.
fn record_frame(
    app: &mut App,
    time: f64,
    fps: u32,
    post_shaders: &[PostProcessShader],
) -> Result<Frame> {
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
    app.update();
    check_custom_animation_errors(app.world())?;
    let Some(blur) = crate::exporter::frame_motion_blur(app.world()) else {
        return capture(app, time, post_shaders, None);
    };
    let mut pins = gaanim_renderer::pipeline::PinnedElements::default();
    // The first pinned capture records the drawables exempt from the blur.
    let mut frame = capture(app, time, post_shaders, Some(&mut pins))?;
    for sample in crate::exporter::motion_blur_times(app.world(), time, f64::from(fps), blur) {
        app.world_mut().resource_mut::<Timeline>().seek_request = Some(sample);
        app.update();
        check_custom_animation_errors(app.world())?;
        frame
            .motion_blur
            .push(capture(app, sample, post_shaders, Some(&mut pins))?);
    }
    Ok(frame)
}

/// Step `app` through the frame at `time` like [`record_frame`] does, with
/// its motion blur sub-frames, without capturing anything: only the state
/// the steps leave behind matters.
fn step_frame(app: &mut App, time: f64, fps: u32) -> Result<()> {
    app.world_mut().resource_mut::<Timeline>().seek_request = Some(time);
    app.update();
    check_custom_animation_errors(app.world())?;
    if let Some(blur) = crate::exporter::frame_motion_blur(app.world()) {
        for sample in crate::exporter::motion_blur_times(app.world(), time, f64::from(fps), blur) {
            app.world_mut().resource_mut::<Timeline>().seek_request = Some(sample);
            app.update();
            check_custom_animation_errors(app.world())?;
        }
    }
    Ok(())
}

fn capture(
    app: &mut App,
    time: f64,
    post_shaders: &[PostProcessShader],
    pins: Option<&mut gaanim_renderer::pipeline::PinnedElements>,
) -> Result<Frame> {
    let camera = frame_camera(app.world())
        .map(|resolved| resolved.camera)
        .ok_or_else(|| ExportError::Capture("the scene has no camera".into()))?;
    let capture = match pins {
        Some(pins) => capture_frame_pinned(app.world_mut(), Some(&camera), pins),
        None => capture_frame(app.world_mut(), Some(&camera)),
    };
    Ok(Frame {
        time,
        camera,
        capture,
        post: post_passes(app.world(), post_shaders, time)?,
        motion_blur: Vec::new(),
    })
}

/// Record the scene that `setup_world_fn` builds into a bundle.
///
/// Callbacks and updaters can keep state that depends on every instant the
/// timeline visits, so the frame grid is recorded in a world that visits
/// exactly the instants an export visits. The instants between grid frames
/// are recorded afterwards in a second world that follows the same grid.
pub fn record_bundle<F>(config: BundleConfig, setup_world_fn: F) -> Result<()>
where
    F: Fn(&mut World) + Clone + Send + Sync + 'static,
{
    let started = Instant::now();
    let telemetry = config.telemetry.clone();

    let mut app = recording_app(setup_world_fn.clone())?;

    if let Some(what) = unsupported_content(app.world_mut()) {
        return Err(ExportError::General(format!(
            "playback bundles cannot record {what} yet; export a video instead"
        )));
    }
    if let Some(mut background) = app.world_mut().get_resource_mut::<CanvasBackground>() {
        background.pixel_size = (config.width, config.height);
    }

    let (plan, segments, markers, polls, poll_session, scenes, duration) = {
        let timeline = app.world().resource::<Timeline>();
        (
            RecordingPlan::new(timeline, config.fps),
            timeline.segments.clone(),
            timeline.markers.clone(),
            timeline.polls.clone(),
            timeline.poll_session.clone(),
            scene_spans(timeline),
            timeline.cached_duration.max(0.0),
        )
    };
    let post_shaders = post_shader_table(app.world().get_resource::<CanvasPostProcess>());
    let background = app.world().get_resource::<CanvasBackground>().cloned();

    if let Some(parent) = config
        .output_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = temporary_path(&config.output_path);
    let mut writer =
        BundleWriter::create(&temporary, format!("gaanim {}", env!("CARGO_PKG_VERSION")))
            .map_err(bundle_error)?;
    let audio = audio_data(&mut writer, app.world())?;

    let total = plan.grid.len() + plan.extras.len();
    // Without state that depends on the instants the timeline visits, the
    // instants between grid frames can be recorded in the same world.
    let single_world =
        plan.extras.is_empty() || (!config.force_second_world && history_free(app.world_mut()));
    // Progress counts every instant the recording visits, including the grid
    // instants a second world steps through without recording them.
    let work = if single_world {
        total
    } else {
        total + plan.second_pass_steps()
    } as u64;
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(work);
    }
    let progress = RecordingProgress::new(work, telemetry);
    let mut cover = config.thumbnail.then(|| {
        ThumbnailPicker::new(ThumbnailPick::new(
            config.thumbnail_time,
            &segments,
            &plan,
            duration,
        ))
    });
    let mut fragments = gaanim_renderer::fragment::FragmentStore::default();
    let mut push = |writer: &mut BundleWriter<_>, frame: Frame| -> Result<()> {
        let digest = gaanim_bundle::frame_digest(&frame, background.as_ref(), &mut fragments);
        if let Some(cover) = &mut cover {
            cover.offer(&frame);
        }
        writer.push_frame(&frame, digest).map_err(bundle_error)?;
        fragments.end_frame();
        Ok(())
    };

    let first_world_times = if single_world {
        plan.times()
    } else {
        plan.grid.clone()
    };
    for time in first_world_times {
        let frame = record_frame(&mut app, time, config.fps, &post_shaders)?;
        push(&mut writer, frame)?;
        progress.advance();
    }
    let clear_color = app.world().get_resource::<ClearColor>().map(|clear| {
        let rgba = clear.0.to_srgba();
        [
            (rgba.red * 255.0) as u8,
            (rgba.green * 255.0) as u8,
            (rgba.blue * 255.0) as u8,
            (rgba.alpha * 255.0) as u8,
        ]
    });

    // Name the elements drawn as poll bars, so presenting the bundle can
    // redraw them at the live votes.
    let poll_bars = {
        let world = app.world_mut();
        let mut bars = world.query::<(Entity, &gaanim_animation::polls::PollBar)>();
        bars.iter(world)
            .map(|(entity, bar)| (entity, bar.clone()))
            .collect::<Vec<_>>()
    }
    .into_iter()
    .map(|(entity, bar)| gaanim_bundle::PollBarRecord {
        key: writer.entity_key(entity),
        poll: bar.poll.to_string(),
        answer: bar.answer,
        preview: bar.preview.to_vec(),
        length: bar.spec.length,
        thickness: bar.spec.thickness,
        radius: bar.spec.radius,
        direction: bar.spec.direction.name().to_string(),
        scale: bar.spec.scale.name().to_string(),
    })
    .collect();

    if !single_world {
        drop(app);
        writer.start_pass().map_err(bundle_error)?;
        let mut app = recording_app(setup_world_fn)?;
        if let Some(mut background) = app.world_mut().get_resource_mut::<CanvasBackground>() {
            background.pixel_size = (config.width, config.height);
        }
        let mut grid = plan.grid.iter().copied().peekable();
        for &extra in &plan.extras {
            // Visit the grid up to the instant as the first world did.
            while let Some(time) = grid.next_if(|time| *time < extra) {
                step_frame(&mut app, time, config.fps)?;
                progress.advance();
            }
            let frame = record_frame(&mut app, extra, config.fps, &post_shaders)?;
            push(&mut writer, frame)?;
            progress.advance();
        }
    }
    progress.finish();

    let scene = SceneData {
        clear_color,
        title: config.title.clone(),
        fps: config.fps,
        duration,
        output_size: (config.width, config.height),
        background,
        post_shaders,
        segments,
        markers,
        scenes,
        audio,
        polls,
        poll_session,
        poll_bars,
    };
    if let Some(frame) = cover.and_then(ThumbnailPicker::into_frame) {
        match render_thumbnail(&scene, &frame) {
            Ok(png) => writer.set_thumbnail(&png).map_err(bundle_error)?,
            Err(error) => console::warn(
                "thumbnail",
                format!("the bundle is recorded without a cover image: {error}"),
            ),
        }
    }
    writer.finish(&scene).map_err(bundle_error)?;
    std::fs::rename(&temporary, &config.output_path)?;
    console::success(
        "done",
        format!(
            "Recorded {total} frames in {:.2}s: {}",
            started.elapsed().as_secs_f64(),
            config.output_path.display()
        ),
    );
    Ok(())
}

/// Longest edge of the cover image, in pixels (as `gaanim_thumbnail::SIZE`).
const THUMBNAIL_SIZE: u32 = 512;

/// Which recorded frame becomes the cover image.
#[derive(Clone, Copy, Debug, PartialEq)]
enum ThumbnailPick {
    /// The frame recorded at this instant.
    At(f64),
    /// The frame of this span that shows the most, the earliest of equals:
    /// the moment the first segment is fully built.
    Fullest { start: f64, end: f64 },
}

impl ThumbnailPick {
    /// `scene.thumbnail(t)` when set; otherwise the frame a presentation
    /// first rests on, since the first frame is usually empty; otherwise
    /// the fullest frame of the first segment.
    fn new(
        explicit: Option<f64>,
        segments: &[gaanim_timeline::timeline::SegmentMetadata],
        plan: &RecordingPlan,
        duration: f64,
    ) -> Self {
        let target = explicit
            .filter(|time| time.is_finite())
            .map(|time| time.clamp(0.0, duration))
            .or_else(|| {
                segments
                    .iter()
                    .flat_map(|segment| segment.stops.iter().map(|stop| stop.time))
                    .filter(|time| time.is_finite())
                    .min_by(f64::total_cmp)
            });
        if let Some(target) = target {
            // The frame a player shows at `target`: the last one at or before it.
            let times = plan.times();
            let shown = times
                .iter()
                .rev()
                .find(|time| **time <= target + 1e-9)
                .or(times.first())
                .copied()
                .unwrap_or(0.0);
            return Self::At(shown);
        }
        match segments.first() {
            Some(segment) => Self::Fullest {
                start: segment.start_time,
                end: segment.end_time,
            },
            None => Self::Fullest {
                start: 0.0,
                end: duration,
            },
        }
    }
}

/// Keeps the recorded frame that becomes the cover image.
struct ThumbnailPicker {
    pick: ThumbnailPick,
    chosen: Option<Frame>,
    score: f64,
}

impl ThumbnailPicker {
    fn new(pick: ThumbnailPick) -> Self {
        Self {
            pick,
            chosen: None,
            score: f64::NEG_INFINITY,
        }
    }

    fn offer(&mut self, frame: &Frame) {
        match self.pick {
            ThumbnailPick::At(time) => {
                if self.chosen.is_none() && (frame.time - time).abs() <= 1e-9 {
                    self.chosen = Some(frame.clone());
                }
            }
            ThumbnailPick::Fullest { start, end } => {
                if frame.time < start - 1e-9 || frame.time > end + 1e-9 {
                    return;
                }
                let score = visible_amount(frame);
                // Frames of a second pass fall between grid frames: an equal
                // score keeps the earlier one.
                let better = score > self.score + 1e-6
                    || ((score - self.score).abs() <= 1e-6
                        && self
                            .chosen
                            .as_ref()
                            .is_some_and(|chosen| frame.time < chosen.time));
                if better {
                    self.score = score;
                    self.chosen = Some(frame.clone());
                }
            }
        }
    }

    fn into_frame(self) -> Option<Frame> {
        self.chosen
    }
}

/// How much a frame shows: the summed opacity of what it draws.
fn visible_amount(frame: &Frame) -> f64 {
    frame
        .capture
        .elements
        .iter()
        .map(|element| f64::from(element.opacity.clamp(0.0, 1.0)))
        .sum()
}

/// PNG of `frame` rendered as an export renders it, its longest edge
/// [`THUMBNAIL_SIZE`] pixels.
fn render_thumbnail(scene: &SceneData, frame: &Frame) -> Result<Vec<u8>> {
    let (width, height) = scene.output_size;
    let longest = width.max(height).max(1);
    let edge = |value: u32| {
        ((u64::from(value) * u64::from(THUMBNAIL_SIZE) + u64::from(longest) / 2)
            / u64::from(longest))
        .max(1) as u32
    };
    let (width, height) = (edge(width), edge(height));
    let mut rasterizer = crate::exporter::FrameRasterizer::new(
        scene,
        width,
        height,
        crate::config::OutputFit::Contain,
    )?;
    let rgba = rasterizer.render(frame, frame.time)?;
    let mut png = Vec::new();
    image::ImageEncoder::write_image(
        image::codecs::png::PngEncoder::new(&mut png),
        &rgba,
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )
    .map_err(|error| ExportError::General(error.to_string()))?;
    Ok(png)
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_default();
    name.push(".partial");
    path.with_file_name(name)
}

fn bundle_error(error: gaanim_bundle::BundleError) -> ExportError {
    ExportError::General(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop, TimelineMarker};

    #[test]
    fn recordings_cover_the_grid_and_every_resting_instant() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 1.05;
        timeline.segments = vec![SegmentMetadata {
            id: 0,
            name: "intro".into(),
            notes: None,
            start_time: 0.0,
            end_time: 1.05,
            stops: vec![SegmentStop {
                name: None,
                time: 0.512,
                ambient: Some(0.25),
            }],
        }];
        timeline.markers = vec![TimelineMarker {
            name: "beat".into(),
            time: 0.333,
        }];
        let times = recording_times(&timeline, 10);
        let expected = [
            0.0, 0.1, 0.2, 0.3, 0.333, 0.4, 0.5, 0.512, 0.6, 0.7, 0.762, 0.8, 0.9, 1.0, 1.05,
        ];
        assert_eq!(times.len(), expected.len(), "{times:?}");
        for (time, expected) in times.iter().zip(expected) {
            assert!((time - expected).abs() < 1e-12, "{times:?}");
        }
        assert!(times.windows(2).all(|pair| pair[0] < pair[1]));
    }

    fn segment(start: f64, end: f64, stops: &[f64]) -> SegmentMetadata {
        SegmentMetadata {
            id: 0,
            name: "s".into(),
            notes: None,
            start_time: start,
            end_time: end,
            stops: stops
                .iter()
                .map(|time| SegmentStop {
                    name: None,
                    time: *time,
                    ambient: None,
                })
                .collect(),
        }
    }

    #[test]
    fn the_cover_is_the_chosen_instant_then_the_first_stop_then_the_fullest_frame() {
        let mut timeline = Timeline::default();
        timeline.cached_duration = 4.0;
        timeline.segments = vec![segment(0.0, 2.0, &[1.512]), segment(2.0, 4.0, &[0.7])];
        let plan = RecordingPlan::new(&timeline, 10);
        let at = |explicit| match ThumbnailPick::new(explicit, &timeline.segments, &plan, 4.0) {
            ThumbnailPick::At(time) => time,
            other => panic!("{other:?}"),
        };
        // The earliest stop of any segment, as recorded (a grid frame here).
        assert!((at(None) - 0.7).abs() < 1e-9);
        // An explicit instant shows the frame recorded at or before it.
        assert_eq!(at(Some(1.55)), 1.512);
        assert_eq!(at(Some(99.0)), 4.0);
        assert_eq!(at(Some(-1.0)), 0.0);
        timeline.segments = vec![segment(0.0, 2.0, &[]), segment(2.0, 4.0, &[])];
        let plan = RecordingPlan::new(&timeline, 10);
        assert_eq!(
            ThumbnailPick::new(None, &timeline.segments, &plan, 4.0),
            ThumbnailPick::Fullest {
                start: 0.0,
                end: 2.0
            }
        );
        assert_eq!(
            ThumbnailPick::new(None, &[], &plan, 4.0),
            ThumbnailPick::Fullest {
                start: 0.0,
                end: 4.0
            }
        );
    }
}
