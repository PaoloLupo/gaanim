//! Plays a recorded bundle (`.gaanim`) in the editor, without Python.
//!
//! The bundle's timeline structure (scenes, segments, stops and markers)
//! drives the usual playback, presentation and navigation. Each frame the
//! frame recorded at the playhead is handed to the renderer as an
//! [`ExternalFrame`], with the camera and post-processing it was recorded
//! with, and the renderer composites it with the same code as the scene.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use gaanim_bundle::Bundle;
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
    /// Post-processing of the shown frame.
    post: Vec<gaanim_bundle::PostPass>,
    /// Set once a frame fails to decode; playback keeps the last good frame.
    failed: bool,
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
    timeline.cached_duration = scene.duration;
    timeline.is_playing = true;
    timeline
}

fn audio_tracks(
    bundle: &mut Bundle,
) -> Result<Vec<gaanim_media::AudioTrack>, gaanim_bundle::BundleError> {
    if bundle.scene.audio.is_empty() {
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
    let mut bundle = Bundle::open(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if bundle.frame_count() == 0 {
        return Err(format!("{} has no frames", path.display()));
    }
    let first = bundle
        .frame(0)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let audio = audio_tracks(&mut bundle).unwrap_or_else(|error| {
        gaanim_core::console::warn(
            "audio",
            format!("the bundle's audio is unavailable: {error}"),
        );
        Vec::new()
    });

    world.insert_resource(bundle_timeline(&bundle));
    world.insert_resource(first.camera);
    if let Some(background) = bundle.scene.background.clone() {
        world.insert_resource(background);
    }
    world.insert_resource(CanvasPostProcess::default());
    world.insert_resource(gaanim_media::PreviewAudioTracks(audio));
    world.insert_resource(ExternalFrame::default());
    if let Some(mut window) = world
        .query_filtered::<&mut Window, With<bevy::window::PrimaryWindow>>()
        .iter_mut(world)
        .next()
    {
        window.title = format!("Gaanim — {}", bundle.scene.title);
    }
    world.insert_resource(BundlePlayback {
        bundle,
        path: path.to_path_buf(),
        shown: None,
        post: Vec::new(),
        failed: false,
    });
    Ok(())
}

/// System: show the recorded frame at the playhead.
pub fn bundle_frame_system(
    mut playback: ResMut<BundlePlayback>,
    timeline: Res<Timeline>,
    mut external: ResMut<ExternalFrame>,
    mut camera: ResMut<gaanim_math::Camera>,
    mut post: ResMut<CanvasPostProcess>,
) {
    let index = playback.bundle.frame_index_at(timeline.current_time);
    if playback.shown == Some(index) || playback.failed {
        return;
    }
    let frame = match playback.bundle.frame(index) {
        Ok(frame) => frame,
        Err(error) => {
            gaanim_core::console::error("bundle", error.to_string());
            playback.failed = true;
            return;
        }
    };
    playback.shown = Some(index);
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
    external.frame = Some(Arc::new(frame.capture));
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
