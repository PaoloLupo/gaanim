//! Plays a recorded bundle (`.gaanim`) in the editor, without Python.
//!
//! The bundle's timeline structure (scenes, segments, stops and markers)
//! drives the usual playback, presentation and navigation. Each frame the
//! frame recorded at the playhead is handed to the renderer as an
//! [`ExternalFrame`], with the camera and post-processing it was recorded
//! with, and the renderer composites it with the same code as the scene.
//!
//! The web player opens a bundle that is still downloading: a frame whose
//! chunk has not arrived keeps the last one on screen, and the player asks
//! the page for the missing bytes ([`BundlePlayback::take_wanted`]).

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use gaanim_bundle::{Bundle, BundleError, BundleSource};
use gaanim_renderer::pipeline::ExternalFrame;
use gaanim_renderer::post_process::{CanvasPostProcess, PostProcessPass};
use gaanim_timeline::clip::ClipPayload;
use gaanim_timeline::timeline::Timeline;

/// Resource: the open bundle and the frame it shows.
#[derive(Resource)]
pub struct BundlePlayback {
    bundle: Bundle,
    path: PathBuf,
    shown: Option<usize>,
    /// Camera of the shown frame. Every playback tick seeks the timeline,
    /// and a seek restores the camera of its t=0 keyframe, so the camera is
    /// reapplied on every tick, not only when the shown frame changes.
    camera: gaanim_math::Camera,
    /// Post-processing of the shown frame.
    post: Vec<gaanim_bundle::PostPass>,
    /// Set once a frame fails to decode; playback keeps the last good frame.
    failed: bool,
    /// Byte ranges of a downloading bundle that playback asked for since the
    /// page last took them.
    wanted: Vec<Range<u64>>,
    /// Whether the frame at the playhead waits for its bytes.
    waiting: bool,
    /// Fragments of the frames composed for previews, apart from playback's.
    preview_store: gaanim_renderer::fragment::FragmentStore,
    /// Elements redrawn from live poll results while presenting.
    live: gaanim_export::live_polls::LiveElements,
    /// What they showed in the frame on screen.
    live_shown: Option<Vec<gaanim_export::live_polls::Shown>>,
}

impl BundlePlayback {
    /// Name the bundle was recorded with.
    pub fn title(&self) -> &str {
        &self.bundle.scene.title
    }

    /// The bundle file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Frames per second the bundle was recorded at.
    pub fn fps(&self) -> u32 {
        self.bundle.scene.fps
    }

    /// Pixel size of the recorded frame, which has the scene's aspect.
    pub fn output_size(&self) -> (u32, u32) {
        self.bundle.scene.output_size
    }

    /// The bundle's audio tracks.
    pub fn audio(&self) -> &[gaanim_bundle::AudioData] {
        &self.bundle.scene.audio
    }

    /// Bytes of the embedded media `entry`; `None` while they download (they
    /// are asked for through [`Self::take_wanted`]).
    pub fn media(&mut self, entry: &str) -> Result<Option<Vec<u8>>, BundleError> {
        match self.bundle.media(entry) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) => match self.want(error) {
                None => Ok(None),
                Some(error) => Err(error),
            },
        }
    }

    /// Byte ranges of a downloading bundle to fetch, and whether the frame at
    /// the playhead waits for them (the rest is read-ahead).
    pub fn take_wanted(&mut self) -> (Vec<Range<u64>>, bool) {
        (std::mem::take(&mut self.wanted), self.waiting)
    }

    /// Note the bytes a read asked for when they are still downloading;
    /// any other error is returned.
    fn want(&mut self, error: BundleError) -> Option<BundleError> {
        match error {
            BundleError::Incomplete { missing } => {
                self.wanted.extend(missing);
                None
            }
            error => Some(error),
        }
    }
}

/// Chunks read ahead of the playhead while a bundle downloads: a few
/// seconds of playback at the default rate.
const READ_AHEAD_CHUNKS: usize = 3;

/// A recorded frame composed for a preview, or why there is none.
pub enum Preview {
    /// The scene and the color to render it over.
    Ready(Box<vello::Scene>, vello::peniko::Color),
    /// Its bytes are still downloading; ask again later.
    Pending,
    Failed,
}

/// A timeline with the bundle's structure and no clips that move anything:
/// playback, stops, ambient loops and chapter navigation work as usual.
fn bundle_timeline(bundle: &Bundle) -> Timeline {
    let scene = &bundle.scene;
    let mut timeline = Timeline::default();
    let track = timeline.add_track("bundle", 0);
    for span in &scene.scenes {
        let id = timeline.add_scene(&span.name);
        timeline.add_clip(track, span.start, 0.0, ClipPayload::SceneStart(id));
        timeline.add_clip(track, span.end, 0.0, ClipPayload::SceneEnd(id));
        timeline.index_scene(id, span.start);
    }
    for segment in &scene.segments {
        for stop in &segment.stops {
            timeline.add_clip(track, stop.time, 0.0, ClipPayload::Stop);
        }
    }
    timeline.set_segments(scene.segments.clone());
    timeline.set_markers(scene.markers.clone());
    timeline.set_polls(scene.polls.clone(), scene.poll_session.clone());
    timeline.set_stop_gates(scene.stop_gates.clone());
    timeline.cached_duration = scene.duration;
    timeline.is_playing = true;
    timeline
}

fn audio_tracks(
    bundle: &mut Bundle,
) -> Result<Vec<gaanim_media::AudioTrack>, gaanim_bundle::BundleError> {
    // Preview audio is decoded from files by FFmpeg, which the web lacks.
    if bundle.scene.audio.is_empty() || cfg!(target_arch = "wasm32") {
        return Ok(Vec::new());
    }
    let dir = std::env::temp_dir().join("gaanim-bundle-media");
    let files = bundle.extract_media(&dir)?;
    Ok(bundle
        .scene
        .audio
        .iter()
        .filter_map(|audio| {
            Some(gaanim_media::AudioTrack {
                path: files.get(&audio.media)?.clone(),
                start_time: audio.start_time,
                duration: audio.duration,
                volume: audio.volume,
                fade_in: audio.fade_in,
                fade_out: audio.fade_out,
                source_offset: audio.source_offset,
                source_duration: audio.source_duration,
                speed: audio.speed,
                looping: audio.looping,
            })
        })
        .collect())
}

/// Open `path` and set the world up to play it.
pub fn open_bundle(world: &mut World, path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    open_bundle_bytes(world, path, bytes.into())
}

/// Set the world up to play the bundle held in `bytes`; `path` names it
/// (the web player has no file system, only the file's name or URL).
pub fn open_bundle_bytes(world: &mut World, path: &Path, bytes: Arc<[u8]>) -> Result<(), String> {
    open_bundle_source(world, path, BundleSource::whole(bytes))
        .map_err(|error| format!("{}: {error}", path.display()))
}

/// Set the world up to play the bundle read from `source`, whose bytes may
/// still be arriving. Fails with [`BundleError::Incomplete`] until the
/// tables and the first chunk have arrived.
pub fn open_bundle_source(
    world: &mut World,
    path: &Path,
    source: BundleSource,
) -> Result<(), BundleError> {
    let mut bundle = Bundle::from_source(source)?;
    if bundle.frame_count() == 0 {
        return Err(BundleError::Corrupt("it has no frames".into()));
    }
    let first = bundle.frame(0)?;
    let audio = audio_tracks(&mut bundle).unwrap_or_else(|error| {
        gaanim_core::console::warn(
            "audio",
            format!("the bundle's audio is unavailable: {error}"),
        );
        Vec::new()
    });

    world.insert_resource(bundle_timeline(&bundle));
    world.insert_resource(gaanim_animation::live::LiveZones(
        bundle.scene.live_zones.clone(),
    ));
    if let Some(rehearsal) = bundle.scene.rehearsal.clone() {
        world.insert_resource(rehearsal);
    }
    world.insert_resource(first.camera);
    if let Some(background) = bundle.scene.background.clone() {
        world.insert_resource(background);
    }
    world.insert_resource(CanvasPostProcess::default());
    world.insert_resource(gaanim_media::PreviewAudioTracks(audio));
    world.insert_resource(ExternalFrame::default());
    // Presenter View previews the recorded frames.
    world.insert_resource(crate::export::StashedReplay {
        canvas: None,
        bundle: Some(crate::export::StashedBundle {
            path: path.to_path_buf(),
            size: bundle.scene.output_size,
        }),
        revision: 1,
    });
    if let Some(mut window) = world
        .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>()
        .iter_mut(world)
        .next()
    {
        window.title = format!("Gaanim — {}", bundle.scene.title);
    }
    let live = gaanim_export::live_polls::LiveElements::from_scene(&bundle.scene);
    world.insert_resource(BundlePlayback {
        live,
        live_shown: None,
        bundle,
        path: path.to_path_buf(),
        shown: None,
        camera: first.camera,
        post: Vec::new(),
        failed: false,
        wanted: Vec::new(),
        waiting: false,
        preview_store: Default::default(),
    });
    Ok(())
}

impl BundlePlayback {
    /// The recorded frame at `time` as a scene `width` by `height` pixels,
    /// with the color to render it over, for Presenter View's cue previews.
    /// Post-processing and shader backgrounds are left out.
    pub fn preview_scene(&mut self, time: f64, width: u32, height: u32) -> Preview {
        let index = self.bundle.frame_index_at(time);
        let frame = match self.bundle.frame(index) {
            Ok(frame) => frame,
            Err(error) => {
                return match self.want(error) {
                    None => Preview::Pending,
                    Some(_) => Preview::Failed,
                };
            }
        };
        let background = self.bundle.scene.background.clone().map(|mut background| {
            background.pixel_size = (width, height);
            // A preview rasterizes a shader background on the CPU, which
            // blocks on the GPU; the web cannot block, so it shows the
            // shader's color.
            if crate::WEB {
                background.paint = without_shader(background.paint);
                for segment in &mut background.segment_paints {
                    segment.paint = segment.paint.take().map(without_shader);
                }
            }
            background
        });
        let scene = gaanim_export::prelude::compose_bundle_frame(
            &frame,
            background.as_ref(),
            &mut self.preview_store,
            width,
            height,
            gaanim_export::config::OutputFit::Contain,
            None,
        );
        let base = self
            .bundle
            .scene
            .clear_color
            .map(|[r, g, b, a]| vello::peniko::Color::from_rgba8(r, g, b, a))
            .unwrap_or(vello::peniko::Color::BLACK);
        Preview::Ready(Box::new(scene), base)
    }
}

/// `paint`, with a shader replaced by its fallback color.
fn without_shader(
    paint: gaanim_renderer::background::BackgroundPaint,
) -> gaanim_renderer::background::BackgroundPaint {
    use gaanim_renderer::background::BackgroundPaint;
    match paint {
        BackgroundPaint::Shader(shader) => BackgroundPaint::solid(shader.fallback()),
        paint => paint,
    }
}

/// System: show the recorded frame at the playhead.
pub fn bundle_frame_system(
    mut playback: ResMut<BundlePlayback>,
    timeline: Res<Timeline>,
    mut external: ResMut<ExternalFrame>,
    mut camera: ResMut<gaanim_math::Camera>,
    mut post: ResMut<CanvasPostProcess>,
    results: Option<Res<gaanim_animation::polls::PollResults>>,
) {
    let index = playback.bundle.frame_index_at(timeline.current_time);
    // Redraw for new votes only when a live element would look different: a
    // quiz's clock changes the results every frame.
    let live_shown = results
        .as_ref()
        .filter(|results| results.is_changed())
        .map_or_else(
            || playback.live_shown.clone(),
            |results| playback.live.shown(results),
        );
    let votes_changed = live_shown != playback.live_shown;
    if (playback.shown == Some(index) && !votes_changed) || playback.failed {
        if *camera != playback.camera {
            *camera = playback.camera;
        }
        return;
    }
    let frame = match playback.bundle.frame(index) {
        Ok(frame) => frame,
        Err(error) => {
            // Bytes still downloading: keep the frame on screen and try again
            // on the next update.
            playback.waiting = true;
            if let Some(error) = playback.want(error) {
                gaanim_core::console::error("bundle", error.to_string());
                playback.failed = true;
            }
            if *camera != playback.camera {
                *camera = playback.camera;
            }
            return;
        }
    };
    playback.waiting = false;
    if !playback.bundle.source().is_complete() {
        let ahead = playback.bundle.missing_around(index, READ_AHEAD_CHUNKS);
        playback.wanted.extend(ahead);
    }
    playback.shown = Some(index);
    playback.camera = frame.camera;
    if *camera != frame.camera {
        *camera = frame.camera;
    }
    if playback.post != frame.post {
        // Recorded uniforms are f32; widening them is exact, so the pass
        // hands the shader the very values recorded.
        let shaders = &playback.bundle.scene.post_shaders;
        post.passes = frame
            .post
            .iter()
            .filter_map(|pass| {
                let shader = shaders.get(pass.shader as usize)?.clone();
                let values: Vec<f64> = pass.values.iter().map(|value| f64::from(*value)).collect();
                PostProcessPass::constant(shader, &values).ok()
            })
            .collect();
        playback.post = frame.post.clone();
    }
    let mut capture = frame.capture;
    if let Some(results) = &results {
        playback.live.apply(results, &mut capture);
    }
    playback.live_shown = live_shown;
    external.frame = Some(Arc::new(capture));
}

/// Plays bundles opened with [`open_bundle`].
pub struct BundlePlayerPlugin;

impl Plugin for BundlePlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            bundle_frame_system
                .run_if(resource_exists::<BundlePlayback>)
                .in_set(gaanim_scene::SceneSet::Updaters),
        );
    }
}

/// Whether `path` names a playback bundle.
pub fn is_bundle_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(gaanim_bundle::EXTENSION))
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use gaanim_bundle::{BundleWriter, Frame, SceneData};

    fn frame(time: f64, zoom: f64) -> Frame {
        let mut camera = gaanim_math::Camera::ortho_2d(1280, 720);
        camera.projection = gaanim_math::Projection::Orthographic { zoom };
        Frame {
            time,
            camera,
            capture: Default::default(),
            post: Vec::new(),
            motion_blur: Vec::new(),
        }
    }

    #[test]
    fn the_recorded_camera_survives_seeks_between_frame_changes() {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        for (index, zoom) in [1.0, 3.0].into_iter().enumerate() {
            writer
                .push_frame(&frame(index as f64 * 0.5, zoom), [0; 32])
                .unwrap();
        }
        let scene = SceneData {
            fps: 2,
            duration: 1.0,
            output_size: (16, 9),
            ..Default::default()
        };
        let bytes = writer.finish(&scene).unwrap().into_inner();
        let mut world = World::new();
        open_bundle_bytes(&mut world, Path::new("test.gaanim"), bytes.into()).unwrap();
        let zoomed = frame(0.5, 3.0).camera;

        world.resource_mut::<Timeline>().current_time = 0.6;
        world.run_system_once(bundle_frame_system).unwrap();
        assert_eq!(*world.resource::<gaanim_math::Camera>(), zoomed);

        // A playback tick seeks, restoring the camera of the t=0 keyframe,
        // while the playhead still shows the same recorded frame.
        world.insert_resource(frame(0.0, 1.0).camera);
        world.resource_mut::<Timeline>().current_time = 0.7;
        world.run_system_once(bundle_frame_system).unwrap();
        assert_eq!(*world.resource::<gaanim_math::Camera>(), zoomed);
    }
}
