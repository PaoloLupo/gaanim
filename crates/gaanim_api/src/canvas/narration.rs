//! Narration authored on a scene.
//!
//! A voiceover block lets a recorded take set the pace: the script names
//! markers and waits for them, and the take (or its estimate, before it is
//! recorded) decides how long that is. A live take instead records a speaker
//! presenting the scene, and replaces every later `stop` with the pause the
//! speaker actually made there.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use gaanim_media::narration::{
    DEFAULT_SCRIPT, MarkerSource, NARRATION_DIR, NarrationError, ScriptSection, TakeFiles,
    TakeSidecar, TakeTiming, audio_duration, parse_script, validate_take_key,
};

use super::SceneModel;
use crate::export::{AudioTrack, AudioTrackError};

static LIVE_TAKE_RECORDING: AtomicBool = AtomicBool::new(false);

/// While set, [`SceneModel::live_take`] ignores recorded takes and keeps
/// stops interactive, so the editor can record a new take over them.
pub fn set_live_take_recording(recording: bool) {
    LIVE_TAKE_RECORDING.store(recording, Ordering::SeqCst);
}

/// Whether the editor is recording a live take.
pub fn live_take_recording() -> bool {
    LIVE_TAKE_RECORDING.load(Ordering::SeqCst)
}

/// A marker a voiceover asked for, in take seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkerSpec {
    pub name: String,
    /// Seconds from the start of the take; `None` when it was not found.
    pub offset: Option<f64>,
    pub source: MarkerSource,
}

/// Where a voiceover's text came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextSource {
    /// The `text` argument.
    Argument,
    /// The take's section of the narration script.
    Script,
    /// The notes of the segment that was active.
    Notes,
    /// No text anywhere.
    Missing,
}

/// One voiceover block of the scene.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceoverSpec {
    pub key: String,
    /// Script shown by the teleprompter and used to estimate timing.
    pub text: Option<String>,
    pub text_source: TextSource,
    /// Segment that was active when the block started.
    pub segment: String,
    /// Absolute timeline time where the take starts.
    pub start_time: f64,
    /// Take length: measured when recorded, estimated otherwise.
    pub duration: f64,
    pub recorded: bool,
    pub transcribed: bool,
    pub files: TakeFiles,
    /// Markers in the order the script resolved them.
    pub markers: Vec<MarkerSpec>,
    /// Loudness, in LUFS, the recorded take was leveled to.
    pub loudness: Option<f64>,
}

/// The scene's live take.
#[derive(Debug, Clone, PartialEq)]
pub struct LiveTakeSpec {
    pub key: String,
    /// Absolute timeline time where the take starts.
    pub start_time: f64,
    /// A recorded take replaced the stops with its holds.
    pub applied: bool,
    /// The editor is recording this take, so stops stayed interactive.
    pub recording: bool,
    /// Length of the applied take.
    pub duration: Option<f64>,
    pub files: TakeFiles,
    /// Seconds spent at each stop by the applied take.
    pub holds: Vec<f64>,
    /// Stops authored after the take started.
    pub stops: usize,
    /// Loudness, in LUFS, the applied take was leveled to.
    pub loudness: Option<f64>,
}

/// The narration script the scene reads its texts from.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptSpec {
    pub path: PathBuf,
    pub sections: Vec<ScriptSection>,
}

impl ScriptSpec {
    /// Text of a take, or `None` when the script has no section for it.
    pub fn text(&self, key: &str) -> Option<&str> {
        self.sections
            .iter()
            .find(|section| section.key == key)
            .map(|section| section.text.as_str())
    }
}

/// Everything the editor needs to record and inspect a scene's narration.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NarrationManifest {
    pub voiceovers: Vec<VoiceoverSpec>,
    pub live: Option<LiveTakeSpec>,
    /// Loaded narration script, if any.
    pub script: Option<ScriptSpec>,
}

impl NarrationManifest {
    pub fn is_empty(&self) -> bool {
        self.voiceovers.is_empty() && self.live.is_none()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NarrationState {
    manifest: NarrationManifest,
    timings: Vec<TakeTiming>,
    finished: Vec<bool>,
    /// Directory holding `narration/` when the scene has no asset root.
    fallback_root: Option<PathBuf>,
    /// The default script was looked for already.
    script_checked: bool,
}

/// Index of a voiceover created by one scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VoiceoverHandle(usize);

#[derive(Debug, thiserror::Error)]
pub enum VoiceoverError {
    #[error(transparent)]
    Take(#[from] NarrationError),
    #[error(transparent)]
    Audio(#[from] AudioTrackError),
    #[error("narration key {key:?} is already used by this scene")]
    DuplicateKey { key: String },
    #[error("this scene already has the live take {key:?}")]
    SecondLiveTake { key: String },
    #[error("volume must be a finite non-negative number")]
    InvalidVolume,
    #[error("voiceover {key:?} has already finished")]
    Finished { key: String },
    #[error("could not read the narration script '{path}': {message}")]
    Script { path: PathBuf, message: String },
}

impl SceneModel {
    /// Directory holding narration takes: `narration/` inside the asset
    /// root, or beside the script when the scene has no asset root.
    pub fn narration_directory(&self) -> PathBuf {
        self.asset_root
            .as_ref()
            .or(self.narration.fallback_root.as_ref())
            .map(|root| root.join(NARRATION_DIR))
            .unwrap_or_else(|| PathBuf::from(NARRATION_DIR))
    }

    /// Base for [`Self::narration_directory`] when no asset root is set.
    pub fn set_narration_fallback_root(&mut self, root: PathBuf) {
        self.narration.fallback_root = Some(root);
    }

    /// Current narration declarations.
    pub fn narration_manifest(&self) -> NarrationManifest {
        self.narration.manifest.clone()
    }

    /// Read voiceover texts from a Markdown script, where each `## key`
    /// section is the text of `voiceover("key")`. A relative path resolves in
    /// the asset directory, or beside the script without one. Voiceovers
    /// started earlier keep their text.
    pub fn narration_script(&mut self, path: impl AsRef<Path>) -> Result<(), VoiceoverError> {
        let path = path.as_ref();
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.asset_root
                .as_ref()
                .or(self.narration.fallback_root.as_ref())
                .map(|root| root.join(path))
                .unwrap_or_else(|| path.to_path_buf())
        };
        let error = |message: String| VoiceoverError::Script {
            path: path.clone(),
            message,
        };
        let source = std::fs::read_to_string(&path).map_err(|e| error(e.to_string()))?;
        let sections = parse_script(&source).map_err(error)?;
        self.narration.script_checked = true;
        self.narration.manifest.script = Some(ScriptSpec { path, sections });
        Ok(())
    }

    /// Load `narration/script.md` the first time narration needs text, unless
    /// a script was chosen explicitly.
    fn ensure_default_script(&mut self) -> Result<(), VoiceoverError> {
        if self.narration.script_checked {
            return Ok(());
        }
        self.narration.script_checked = true;
        let path = self.narration_directory().join(DEFAULT_SCRIPT);
        if path.is_file() {
            self.narration_script(path)?;
        }
        Ok(())
    }

    fn ensure_unused_key(&self, key: &str) -> Result<(), VoiceoverError> {
        validate_take_key(key)?;
        let manifest = &self.narration.manifest;
        if manifest.voiceovers.iter().any(|voice| voice.key == key)
            || manifest.live.as_ref().is_some_and(|live| live.key == key)
        {
            return Err(VoiceoverError::DuplicateKey {
                key: key.to_owned(),
            });
        }
        Ok(())
    }

    /// Start a voiceover block at the cursor.
    ///
    /// A recorded take `narration/<key>.*` plays from here and sets the
    /// block's length; otherwise the length is estimated from its text: the
    /// `text` argument, else the key's section of the narration script, else
    /// the active segment's notes.
    pub fn voiceover(
        &mut self,
        key: &str,
        text: Option<String>,
        volume: f64,
    ) -> Result<VoiceoverHandle, VoiceoverError> {
        self.ensure_unused_key(key)?;
        if !volume.is_finite() || volume < 0.0 {
            return Err(VoiceoverError::InvalidVolume);
        }
        self.ensure_default_script()?;
        let (segment, notes) = {
            let state = self.state.lock().expect("canvas state poisoned");
            let active = state.active();
            (active.name.clone(), active.notes.clone())
        };
        let written = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        let script = written(
            self.narration
                .manifest
                .script
                .as_ref()
                .and_then(|script| script.text(key))
                .map(str::to_owned),
        );
        let (text, text_source) = match (written(text), script, written(notes)) {
            (Some(text), _, _) => (Some(text), TextSource::Argument),
            (None, Some(text), _) => (Some(text), TextSource::Script),
            (None, None, Some(text)) => (Some(text), TextSource::Notes),
            (None, None, None) => (None, TextSource::Missing),
        };
        let files = TakeFiles::locate(&self.narration_directory(), key);
        let recorded_duration = files.audio.as_deref().map(audio_duration).transpose()?;
        let sidecar = if recorded_duration.is_some() {
            TakeSidecar::load(&files.sidecar)?
        } else {
            TakeSidecar::default()
        };
        let loudness = sidecar.loudness;
        let timing = TakeTiming::new(recorded_duration, sidecar, text.as_deref());
        let start_time = self.current_time();
        if let (Some(path), Some(duration)) = (&files.audio, recorded_duration) {
            self.audio_tracks.push(AudioTrack::new(
                path.clone(),
                start_time,
                Some(duration),
                volume,
                0.0,
                0.0,
            )?);
        }
        let narration = &mut self.narration;
        narration.manifest.voiceovers.push(VoiceoverSpec {
            key: key.to_owned(),
            text,
            text_source,
            segment,
            start_time,
            duration: timing.duration,
            recorded: timing.recorded,
            transcribed: timing.transcribed,
            files,
            markers: Vec::new(),
            loudness,
        });
        narration.timings.push(timing);
        narration.finished.push(false);
        Ok(VoiceoverHandle(narration.timings.len() - 1))
    }

    pub fn voiceover_spec(&self, handle: VoiceoverHandle) -> &VoiceoverSpec {
        &self.narration.manifest.voiceovers[handle.0]
    }

    fn ensure_open(&self, handle: VoiceoverHandle) -> Result<(), VoiceoverError> {
        if self.narration.finished[handle.0] {
            return Err(VoiceoverError::Finished {
                key: self.voiceover_spec(handle).key.clone(),
            });
        }
        Ok(())
    }

    /// Absolute time of a marker, or `None` when it cannot be found. The
    /// first request for a name searches after the previous marker; later
    /// requests for the same name reuse that answer.
    pub fn voiceover_marker(
        &mut self,
        handle: VoiceoverHandle,
        name: &str,
    ) -> Result<(Option<f64>, MarkerSource), VoiceoverError> {
        self.ensure_open(handle)?;
        let narration = &mut self.narration;
        let spec = &mut narration.manifest.voiceovers[handle.0];
        let marker = match spec.markers.iter().find(|marker| marker.name == name) {
            Some(marker) => marker.clone(),
            None => {
                let (offset, source) = narration.timings[handle.0].resolve(name);
                let marker = MarkerSpec {
                    name: name.to_owned(),
                    offset,
                    source,
                };
                spec.markers.push(marker.clone());
                marker
            }
        };
        Ok((
            marker.offset.map(|offset| spec.start_time + offset),
            marker.source,
        ))
    }

    /// Advance the cursor to a marker. A marker that is already behind the
    /// cursor, or missing, leaves the cursor where it is.
    pub fn voiceover_wait_until(
        &mut self,
        handle: VoiceoverHandle,
        name: &str,
    ) -> Result<MarkerSource, VoiceoverError> {
        let (time, source) = self.voiceover_marker(handle, name)?;
        if let Some(time) = time {
            let now = self.current_time();
            if time > now {
                self.wait(time - now);
            }
        }
        Ok(source)
    }

    /// Seconds from the cursor to a marker, never negative.
    pub fn voiceover_until(
        &mut self,
        handle: VoiceoverHandle,
        name: &str,
    ) -> Result<(f64, MarkerSource), VoiceoverError> {
        let (time, source) = self.voiceover_marker(handle, name)?;
        let now = self.current_time();
        Ok((time.map_or(0.0, |time| (time - now).max(0.0)), source))
    }

    /// Seconds from the cursor to the end of the take, never negative.
    pub fn voiceover_remaining(&self, handle: VoiceoverHandle) -> f64 {
        let spec = self.voiceover_spec(handle);
        (spec.start_time + spec.duration - self.current_time()).max(0.0)
    }

    /// Wait for the rest of the take and close the block. Finishing twice
    /// does nothing.
    pub fn finish_voiceover(&mut self, handle: VoiceoverHandle) {
        if self.narration.finished[handle.0] {
            return;
        }
        let remaining = self.voiceover_remaining(handle);
        if remaining > 0.0 {
            self.wait(remaining);
        }
        self.narration.finished[handle.0] = true;
    }

    /// Start the scene's live take at the cursor.
    ///
    /// When `narration/<key>.*` exists, it plays from here and every later
    /// [`Self::stop`] becomes the hold recorded for it. Without a take, or
    /// while the editor records one, stops stay interactive.
    pub fn live_take(&mut self, key: &str, volume: f64) -> Result<(), VoiceoverError> {
        if let Some(live) = &self.narration.manifest.live {
            return Err(VoiceoverError::SecondLiveTake {
                key: live.key.clone(),
            });
        }
        self.ensure_unused_key(key)?;
        if !volume.is_finite() || volume < 0.0 {
            return Err(VoiceoverError::InvalidVolume);
        }
        // The editor's live teleprompter reads sections named after segments.
        self.ensure_default_script()?;
        let files = TakeFiles::locate(&self.narration_directory(), key);
        let recording = live_take_recording();
        let start_time = self.current_time();
        let (duration, holds, loudness) = match (&files.audio, recording) {
            (Some(path), false) => {
                let duration = audio_duration(path)?;
                let sidecar = TakeSidecar::load(&files.sidecar)?;
                self.audio_tracks.push(AudioTrack::new(
                    path.clone(),
                    start_time,
                    Some(duration),
                    volume,
                    0.0,
                    0.0,
                )?);
                (Some(duration), sidecar.holds, sidecar.loudness)
            }
            _ => (None, Vec::new(), None),
        };
        self.narration.manifest.live = Some(LiveTakeSpec {
            key: key.to_owned(),
            start_time,
            applied: duration.is_some(),
            recording,
            duration,
            files,
            holds,
            stops: 0,
            loudness,
        });
        Ok(())
    }

    /// Count a stop authored after the live take started. Returns the hold
    /// that replaces it when a recorded take is applied.
    pub(crate) fn live_take_hold(&mut self) -> Option<f64> {
        let live = self.narration.manifest.live.as_mut()?;
        let index = live.stops;
        live.stops += 1;
        live.applied
            .then(|| live.holds.get(index).copied().unwrap_or(0.0))
    }

    /// Absolute time at which every narration take has finished.
    pub fn narration_end(&self) -> f64 {
        let manifest = &self.narration.manifest;
        let voices = manifest
            .voiceovers
            .iter()
            .map(|voice| voice.start_time + voice.duration);
        let live = manifest
            .live
            .iter()
            .filter_map(|live| Some(live.start_time + live.duration?));
        voices.chain(live).fold(0.0, f64::max)
    }

    /// Close open voiceovers and extend the timeline so no take is cut off.
    pub fn complete_narration(&mut self) {
        let end = self.narration_end();
        let now = self.current_time();
        if end > now + 1e-9 {
            self.wait(end - now);
        }
        self.narration.finished.fill(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_media::narration::{DEFAULT_SCRIPT, write_wav_take};

    fn scene_with_assets(root: &std::path::Path) -> SceneModel {
        let mut scene = SceneModel::new(16, 9);
        scene.set_asset_root(root).unwrap();
        scene
    }

    fn record(root: &std::path::Path, key: &str, seconds: f64, sidecar: TakeSidecar) {
        let files = TakeFiles::locate(&root.join(NARRATION_DIR), key);
        let samples = vec![0.0_f32; (seconds * 8_000.0) as usize];
        write_wav_take(&files.destination, &samples, 8_000).unwrap();
        sidecar.save(&files.sidecar).unwrap();
    }

    #[test]
    fn unrecorded_voiceover_is_timed_by_its_script() {
        let root = tempfile::tempdir().unwrap();
        let mut scene = scene_with_assets(root.path());
        scene.wait(1.0);
        let text = "uno dos tres cuatro cinco seis siete ocho nueve diez";
        let voice = scene.voiceover("intro", Some(text.into()), 1.0).unwrap();
        assert!(scene.audio_tracks.is_empty(), "no take, no audio");
        assert_eq!(
            scene.voiceover_wait_until(voice, "seis").unwrap(),
            MarkerSource::Estimated
        );
        assert!((scene.current_time() - 3.0).abs() < 1e-9);
        // Asking again for the same marker reuses its time.
        assert_eq!(scene.voiceover_until(voice, "seis").unwrap().0, 0.0);
        assert_eq!(
            scene.voiceover_wait_until(voice, "once").unwrap(),
            MarkerSource::Missing
        );
        scene.finish_voiceover(voice);
        assert!((scene.current_time() - 5.4).abs() < 1e-9);
        assert!(matches!(
            scene.voiceover_wait_until(voice, "diez"),
            Err(VoiceoverError::Finished { .. })
        ));
        let manifest = scene.narration_manifest();
        assert_eq!(manifest.voiceovers[0].markers.len(), 2);
        assert!(!manifest.voiceovers[0].recorded);
    }

    #[test]
    fn recorded_take_plays_at_the_block_and_drives_its_markers() {
        let root = tempfile::tempdir().unwrap();
        let mut sidecar = TakeSidecar::default();
        sidecar.markers.insert("recta".into(), 1.5);
        record(root.path(), "intro", 4.0, sidecar);

        let mut scene = scene_with_assets(root.path());
        scene.segment("inicio", None).unwrap();
        scene.wait(0.5);
        let voice = scene
            .voiceover("intro", Some("la recta".into()), 0.8)
            .unwrap();
        assert_eq!(scene.audio_tracks.len(), 1);
        assert_eq!(scene.audio_tracks[0].start_time, 0.5);
        assert_eq!(scene.audio_tracks[0].duration, Some(4.0));
        assert_eq!(scene.audio_tracks[0].volume, 0.8);
        let (until, source) = scene.voiceover_until(voice, "recta").unwrap();
        assert_eq!((until, source), (1.5, MarkerSource::Tapped));
        scene.voiceover_wait_until(voice, "recta").unwrap();
        assert!((scene.current_time() - 2.0).abs() < 1e-9);
        assert!((scene.voiceover_remaining(voice) - 2.5).abs() < 1e-9);
        scene.finish_voiceover(voice);
        scene.finish_voiceover(voice);
        assert!((scene.current_time() - 4.5).abs() < 1e-9);
        assert_eq!(scene.narration_manifest().voiceovers[0].segment, "inicio");
    }

    #[test]
    fn voiceover_text_defaults_to_segment_notes() {
        let root = tempfile::tempdir().unwrap();
        let mut scene = scene_with_assets(root.path());
        scene
            .segment_with("tema", None, Some("uno dos tres".into()), None)
            .unwrap();
        scene.voiceover("tema", None, 1.0).unwrap();
        let manifest = scene.narration_manifest();
        assert_eq!(manifest.voiceovers[0].text.as_deref(), Some("uno dos tres"));
        assert!(matches!(
            scene.voiceover("tema", None, 1.0),
            Err(VoiceoverError::DuplicateKey { .. })
        ));
        assert!(matches!(
            scene.voiceover("a/b", None, 1.0),
            Err(VoiceoverError::Take(NarrationError::InvalidKey { .. }))
        ));
    }

    #[test]
    fn recorded_live_take_replaces_later_stops_with_its_holds() {
        let root = tempfile::tempdir().unwrap();
        let sidecar = TakeSidecar {
            holds: vec![2.0, 0.75],
            ..TakeSidecar::default()
        };
        record(root.path(), "clase", 9.0, sidecar);

        let mut scene = scene_with_assets(root.path());
        scene.stop(None).unwrap_or(()); // before the take: an ordinary stop
        scene.wait(1.0);
        scene.live_take("clase", 1.0).unwrap();
        scene.wait(1.0);
        scene.stop(Some("a".into())).unwrap();
        scene.wait(1.0);
        scene.stop(None).unwrap();
        scene.stop(None).unwrap(); // more stops than holds: no pause
        assert!((scene.current_time() - 5.75).abs() < 1e-9);
        let stops: usize = scene
            .segment_manifest()
            .segments
            .iter()
            .map(|segment| segment.stops.len())
            .sum();
        assert_eq!(stops, 1, "only the stop before the take remains");
        assert_eq!(scene.audio_tracks[0].start_time, 1.0);

        // The timeline is extended so the take is not cut off.
        scene.complete_narration();
        assert!((scene.current_time() - 10.0).abs() < 1e-9);
        let live = scene.narration_manifest().live.unwrap();
        assert!(live.applied && !live.recording);
        assert_eq!(live.stops, 3);
    }

    #[test]
    fn live_take_keeps_stops_without_a_take_or_while_recording() {
        let root = tempfile::tempdir().unwrap();
        let mut scene = scene_with_assets(root.path());
        scene.live_take("clase", 1.0).unwrap();
        scene.wait(1.0);
        scene.stop(None).unwrap();
        assert!((scene.current_time() - 1.0).abs() < 1e-9);
        assert!(matches!(
            scene.live_take("otra", 1.0),
            Err(VoiceoverError::SecondLiveTake { .. })
        ));
        let live = scene.narration_manifest().live.unwrap();
        assert!(!live.applied);
        assert_eq!(live.stops, 1);
    }

    #[test]
    fn voiceover_text_comes_from_the_default_script() {
        let root = tempfile::tempdir().unwrap();
        let narration = root.path().join(NARRATION_DIR);
        std::fs::create_dir_all(&narration).unwrap();
        std::fs::write(
            narration.join(DEFAULT_SCRIPT),
            "# Guion\n\n## intro\nuno dos tres cuatro cinco\n\n## vacio\n",
        )
        .unwrap();
        let mut scene = scene_with_assets(root.path());
        scene
            .segment_with("tema", None, Some("notas del segmento".into()), None)
            .unwrap();
        let intro = scene.voiceover("intro", None, 1.0).unwrap();
        let spec = scene.voiceover_spec(intro);
        assert_eq!(spec.text.as_deref(), Some("uno dos tres cuatro cinco"));
        assert_eq!(spec.text_source, TextSource::Script);
        assert!((spec.duration - 2.4).abs() < 1e-9);
        // An explicit text wins; an empty section falls back to the notes.
        let explicit = scene.voiceover("otra", Some("hola".into()), 1.0).unwrap();
        assert_eq!(
            scene.voiceover_spec(explicit).text_source,
            TextSource::Argument
        );
        let empty = scene.voiceover("vacio", None, 1.0).unwrap();
        assert_eq!(scene.voiceover_spec(empty).text_source, TextSource::Notes);
        let script = scene.narration_manifest().script.unwrap();
        assert_eq!(script.sections.len(), 2);
        assert_eq!(script.text("intro"), Some("uno dos tres cuatro cinco"));
    }

    #[test]
    fn scripts_load_from_an_explicit_path_and_report_errors() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("guion.md"), "## intro\nhola mundo\n").unwrap();
        std::fs::write(root.path().join("roto.md"), "## dos palabras\nx\n").unwrap();
        let mut scene = scene_with_assets(root.path());
        scene.narration_script("guion.md").unwrap();
        let voice = scene.voiceover("intro", None, 1.0).unwrap();
        assert_eq!(
            scene.voiceover_spec(voice).text.as_deref(),
            Some("hola mundo")
        );
        assert!(matches!(
            scene.narration_script("roto.md"),
            Err(VoiceoverError::Script { .. })
        ));
        assert!(matches!(
            scene.narration_script("no-existe.md"),
            Err(VoiceoverError::Script { .. })
        ));
    }
}
