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
        .query_filtered::<(), With<gaanim_scene::Mesh3DMarker>>()
        .iter(world)
        .next()
        .is_some()
        || world
            .query_filtered::<(), With<gaanim_scene::GltfModelRoot>>()
            .iter(world)
            .next()
            .is_some()
    {
        return Some("3D content (meshes, surfaces and glTF models)");
    }
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

    let (plan, segments, markers, scenes, duration) = {
        let timeline = app.world().resource::<Timeline>();
        (
            RecordingPlan::new(timeline, config.fps),
            timeline.segments.clone(),
            timeline.markers.clone(),
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
    if let Some(telemetry) = &telemetry {
        telemetry.set_total_frames(total as u64);
    }
    let progress = crate::exporter::create_progress_bar(total as u64);
    let mut fragments = gaanim_renderer::fragment::FragmentStore::default();
    let mut recorded = 0_u64;
    let mut push = |writer: &mut BundleWriter<_>, frame: Frame| -> Result<()> {
        let digest = gaanim_bundle::frame_digest(&frame, background.as_ref(), &mut fragments);
        writer.push_frame(&frame, digest).map_err(bundle_error)?;
        fragments.end_frame();
        recorded += 1;
        progress.inc(1);
        if let Some(telemetry) = &telemetry {
            telemetry.set_current_frame(recorded);
        }
        Ok(())
    };

    for &time in &plan.grid {
        let frame = record_frame(&mut app, time, config.fps, &post_shaders)?;
        push(&mut writer, frame)?;
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

    if !plan.extras.is_empty() {
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
                record_frame(&mut app, time, config.fps, &post_shaders)?;
            }
            let frame = record_frame(&mut app, extra, config.fps, &post_shaders)?;
            push(&mut writer, frame)?;
        }
    }
    progress.finish_and_clear();

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
    };
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
}
