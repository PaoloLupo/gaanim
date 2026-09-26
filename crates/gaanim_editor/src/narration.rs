//! Narration recorder.
//!
//! Records the voice for a scene without leaving the editor. A voiceover
//! take is recorded while its block plays, with its script as teleprompter;
//! pressing Space when saying a marker stores that instant. A live take is
//! recorded by presenting the scene: playback pauses at each `stop()` and
//! the time spent there becomes a hold that replaces the stop. Takes and
//! their sidecars land in the scene's `narration/` folder, leveled to a
//! standard voice loudness, and the file watcher reloads the scene with the
//! new timing. Texts can live in a Markdown script edited outside the code.

mod level;
mod recorder;
mod settings;
mod whisper;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::{
    MarkerSource, NarrationManifest, TakeFiles, TextSource, VoiceoverSpec, set_live_take_recording,
};
use gaanim_media::narration::{
    DEFAULT_SCRIPT, TakeSidecar, format_script, normalize_word, parse_script, write_atomically,
    write_wav_take,
};
use gaanim_timeline::timeline::{PlaybackStopPolicy, Timeline};

use crate::export::StashedReplay;
use crate::ui_kit::{
    ButtonTone, Icon, caption, card_frame, field_frame, icon_button, paint_icon, palette,
    primary_button, secondary_button, section_label, small_button,
};
use crate::{EditorState, PresentationMode};
use level::{Leveling, start_leveling};
use recorder::MicRecorder;
use settings::NarrationSettings;
use whisper::{Transcription, start_transcription};

const COUNTDOWN: Duration = Duration::from_secs(3);
const LIVE_RELOAD_TIMEOUT: Duration = Duration::from_secs(20);
/// Shorter recordings are treated as accidental and discarded.
const MIN_TAKE_SECONDS: f64 = 0.3;
/// Beside the takes, the recordings as captured, before leveling. Hidden so
/// the file watcher does not reload the scene for them.
const ORIGINALS_DIR: &str = ".originals";

/// Peaks of a spoken take belong between these levels, in dBFS: clear of
/// the noise floor, with headroom for a louder word.
const IDEAL_PEAK_DB: (f32, f32) = (-18.0, -6.0);
/// Above this, a louder word may clip.
const HOT_PEAK_DB: f32 = -3.0;
/// Quieter than this for [`QUIET_AFTER`], the voice is too far or too low.
const QUIET_PEAK_DB: f32 = -30.0;
const QUIET_AFTER: Duration = Duration::from_secs(4);
/// How long a hot peak keeps the warning on screen.
const HOT_WARNING: Duration = Duration::from_millis(1500);
/// Range drawn by the input meter, in dBFS.
const METER_FLOOR_DB: f32 = -48.0;

fn to_db(linear: f32) -> f32 {
    20.0 * linear.max(1e-5).log10()
}

/// What the input meter tells the speaker.
#[derive(Debug, Clone, PartialEq)]
enum LevelWarning {
    /// The last peaks came close to clipping.
    Hot,
    /// Part of the take clipped; that distortion cannot be undone.
    Clipped(u32),
    /// Nothing reached a usable level for a while.
    Quiet,
}

impl LevelWarning {
    fn message(&self) -> String {
        match self {
            Self::Hot => "¡Muy fuerte! Baja la ganancia o aléjate del micrófono".to_string(),
            Self::Clipped(1) => {
                "Saturó 1 vez: esa distorsión no se corrige; considera repetir la toma".to_string()
            }
            Self::Clipped(count) => format!(
                "Saturó {count} veces: esa distorsión no se corrige; considera repetir la toma"
            ),
            Self::Quiet => "Voz muy baja: acércate al micrófono o sube la ganancia".to_string(),
        }
    }

    fn color(&self) -> egui::Color32 {
        match self {
            Self::Hot | Self::Clipped(_) => palette::DANGER,
            Self::Quiet => palette::STOP,
        }
    }
}

/// Input level of the open microphone, as the meter shows it.
#[derive(Debug, Clone, Copy)]
struct InputLevel {
    /// Displayed peak, falling back smoothly, from 0 to 1.
    peak: f32,
    hot_until: Option<Instant>,
    /// Last time the voice reached a usable level.
    heard_at: Instant,
}

impl InputLevel {
    fn new(now: Instant) -> Self {
        Self {
            peak: 0.0,
            hot_until: None,
            heard_at: now,
        }
    }

    fn update(&mut self, peak: f32, now: Instant) {
        self.peak = (self.peak * 0.85).max(peak);
        let db = to_db(peak);
        if db >= HOT_PEAK_DB {
            self.hot_until = Some(now + HOT_WARNING);
        }
        if db >= QUIET_PEAK_DB {
            self.heard_at = now;
        }
    }

    fn warning(&self, clipped: u32, now: Instant) -> Option<LevelWarning> {
        if self.hot_until.is_some_and(|until| now < until) {
            Some(LevelWarning::Hot)
        } else if clipped > 0 {
            Some(LevelWarning::Clipped(clipped))
        } else if now.duration_since(self.heard_at) > QUIET_AFTER {
            Some(LevelWarning::Quiet)
        } else {
            None
        }
    }
}

/// Color of a peak on the input meter.
fn peak_color(db: f32) -> egui::Color32 {
    if db >= HOT_PEAK_DB {
        palette::DANGER
    } else if db > IDEAL_PEAK_DB.1 {
        palette::STOP
    } else if db >= IDEAL_PEAK_DB.0 {
        palette::LOOP
    } else {
        palette::TEXT_MUTED
    }
}

/// The open microphone and its level guidance.
struct Input {
    recorder: MicRecorder,
    level: InputLevel,
}

impl Input {
    fn update(&mut self, now: Instant) {
        let peak = self.recorder.take_peak();
        self.level.update(peak, now);
    }

    fn warning(&self, now: Instant) -> Option<LevelWarning> {
        self.level.warning(self.recorder.clipped(), now)
    }
}

/// Background jobs and the recorder rewrite sidecars one at a time.
static SIDECAR_LOCK: Mutex<()> = Mutex::new(());

/// Read, change and rewrite a take's sidecar.
pub(crate) fn update_sidecar(
    path: &Path,
    change: impl FnOnce(&mut TakeSidecar),
) -> Result<(), String> {
    let _guard = SIDECAR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut sidecar = TakeSidecar::load(path).map_err(|error| error.to_string())?;
    change(&mut sidecar);
    sidecar.save(path).map_err(|error| error.to_string())
}

fn replace_sidecar(path: &Path, sidecar: &TakeSidecar) -> Result<(), String> {
    let _guard = SIDECAR_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sidecar.save(path).map_err(|error| error.to_string())
}

/// Run a tool; on failure, report the last line it printed.
pub(crate) fn run_tool(command: &mut Command) -> Result<Output, String> {
    let output = command.output().map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(output)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().rev().find(|line| !line.trim().is_empty());
        Err(detail.unwrap_or("sin detalles").trim().to_string())
    }
}

/// A fresh temporary directory for one background job.
pub(crate) fn scratch_dir(job: &str, key: &str) -> Result<PathBuf, String> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let directory =
        std::env::temp_dir().join(format!("gaanim-{job}-{}-{key}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn originals_dir(files: &TakeFiles) -> PathBuf {
    files
        .sidecar
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(ORIGINALS_DIR)
}

/// Re-runs the scene script. Installed by the application host, which owns
/// the script runner.
#[derive(Resource, Clone)]
pub struct ScriptReload(pub Arc<dyn Fn() + Send + Sync>);

/// The narration panel and requests made from it.
#[derive(Resource, Default)]
pub struct NarrationPanel {
    pub open: bool,
    settings: Option<NarrationSettings>,
    notice: Option<(String, NoticeKind)>,
    transcriptions: Vec<Transcription>,
    levelings: Vec<Leveling>,
    request: Option<Request>,
    /// The teleprompter shows only its status line.
    compact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoticeKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
enum Request {
    RecordVoiceover(String),
    RecordLive,
    Listen(f64),
    Transcribe(String),
    Level(String),
    OpenScript,
}

impl NarrationPanel {
    fn settings(&mut self) -> &mut NarrationSettings {
        self.settings.get_or_insert_with(NarrationSettings::load)
    }

    fn notify(&mut self, kind: NoticeKind, message: impl Into<String>) {
        let message = message.into();
        let line = format!("narration: {message}");
        match kind {
            NoticeKind::Info => gaanim_core::console::info(line),
            NoticeKind::Success => gaanim_core::console::success(line),
            NoticeKind::Error => gaanim_core::console::error(line),
        }
        self.notice = Some((message, kind));
    }

    fn transcribing(&self, key: &str) -> bool {
        self.transcriptions.iter().any(|job| job.key == key)
    }

    fn leveling(&self, key: &str) -> bool {
        self.levelings.iter().any(|job| job.key == key)
    }

    /// A background job is changing this take's files.
    fn busy(&self, key: &str) -> bool {
        self.transcribing(key) || self.leveling(key)
    }

    fn transcribe(&mut self, key: &str, take: PathBuf, sidecar: PathBuf) {
        if self.transcribing(key) {
            return;
        }
        let settings = self.settings().clone();
        match start_transcription(key.to_owned(), take, sidecar, &settings) {
            Ok(job) => {
                self.transcriptions.push(job);
                self.notify(
                    NoticeKind::Info,
                    format!("Transcribiendo «{key}» con Whisper…"),
                );
            }
            Err(error) => self.notify(NoticeKind::Error, format!("Whisper: {error}")),
        }
    }

    /// Level a take that was already recorded, keeping its first version.
    fn level_existing(&mut self, key: &str, files: &TakeFiles) {
        if self.busy(key) {
            return;
        }
        let Some(audio) = files.audio.clone() else {
            self.notify(NoticeKind::Error, format!("«{key}» todavía no tiene toma"));
            return;
        };
        let originals = originals_dir(files);
        let original = match TakeFiles::locate(&originals, key).audio {
            Some(original) => original,
            None => {
                let extension = audio
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or("wav");
                let original = originals.join(format!("{key}.{extension}"));
                let copied = std::fs::create_dir_all(&originals)
                    .and_then(|()| std::fs::copy(&audio, &original));
                if let Err(error) = copied {
                    self.notify(
                        NoticeKind::Error,
                        format!("no se pudo guardar el original de «{key}»: {error}"),
                    );
                    return;
                }
                original
            }
        };
        let target = self.settings().loudness;
        self.start_leveling(key, original, files, target);
    }

    fn start_leveling(&mut self, key: &str, original: PathBuf, files: &TakeFiles, target: f64) {
        match start_leveling(
            key.to_owned(),
            original,
            files.destination.clone(),
            files.sidecar.clone(),
            target,
        ) {
            Ok(job) => {
                self.levelings.push(job);
                self.notify(
                    NoticeKind::Info,
                    format!("Nivelando «{key}» a {target:.0} LUFS…"),
                );
            }
            Err(error) => self.notify(NoticeKind::Error, format!("no se pudo nivelar: {error}")),
        }
    }

    fn poll_jobs(&mut self) {
        let mut finished = Vec::new();
        self.transcriptions.retain(|job| match job.poll() {
            Some(result) => {
                finished.push(match result {
                    Ok(words) => (
                        NoticeKind::Success,
                        format!("«{}» transcrita: {words} palabras con tiempo", job.key),
                    ),
                    Err(error) => (
                        NoticeKind::Error,
                        format!("Whisper («{}»): {error}", job.key),
                    ),
                });
                false
            }
            None => true,
        });
        let target = self
            .settings
            .as_ref()
            .map_or(settings::DEFAULT_LOUDNESS, |s| s.loudness);
        self.levelings.retain(|job| match job.poll() {
            Some(result) => {
                finished.push(match result {
                    Ok(measured) => (
                        NoticeKind::Success,
                        format!(
                            "«{}» nivelada a {target:.0} LUFS (estaba en {:.1})",
                            job.key, measured.integrated
                        ),
                    ),
                    Err(error) => (
                        NoticeKind::Error,
                        format!("«{}» se guardó sin nivelar: {error}", job.key),
                    ),
                });
                false
            }
            None => true,
        });
        for (kind, message) in finished {
            self.notify(kind, message);
        }
    }
}

/// What is being recorded.
#[derive(Debug, Clone)]
enum Target {
    Voiceover {
        key: String,
        start_time: f64,
        text: Option<String>,
        markers: Vec<String>,
        files: TakeFiles,
    },
    Live {
        key: String,
        start_time: f64,
        files: TakeFiles,
        /// Script sections, shown for the segment of the same name.
        script: Vec<(String, String)>,
    },
}

impl Target {
    fn voiceover(spec: &VoiceoverSpec) -> Self {
        Self::Voiceover {
            key: spec.key.clone(),
            start_time: spec.start_time,
            text: spec.text.clone(),
            markers: spec
                .markers
                .iter()
                .map(|marker| marker.name.clone())
                .collect(),
            files: spec.files.clone(),
        }
    }

    fn live(manifest: &NarrationManifest) -> Option<Self> {
        let live = manifest.live.as_ref()?;
        Some(Self::Live {
            key: live.key.clone(),
            start_time: live.start_time,
            files: live.files.clone(),
            script: manifest
                .script
                .iter()
                .flat_map(|script| &script.sections)
                .map(|section| (section.key.clone(), section.text.clone()))
                .collect(),
        })
    }

    fn key(&self) -> &str {
        match self {
            Self::Voiceover { key, .. } | Self::Live { key, .. } => key,
        }
    }

    fn start_time(&self) -> f64 {
        match self {
            Self::Voiceover { start_time, .. } | Self::Live { start_time, .. } => *start_time,
        }
    }

    fn files(&self) -> &TakeFiles {
        match self {
            Self::Voiceover { files, .. } | Self::Live { files, .. } => files,
        }
    }

    fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }
}

struct Recording {
    target: Target,
    input: Input,
    /// Voiceover markers tapped so far, in take seconds.
    taps: Vec<(String, f64)>,
    /// Live take: seconds spent at each stop passed so far.
    holds: Vec<f64>,
    /// Live take: timeline time of the stop playback is waiting at.
    waiting_at: Option<f64>,
}

impl Recording {
    fn next_marker(&self) -> Option<&str> {
        match &self.target {
            Target::Voiceover { markers, .. } => markers.get(self.taps.len()).map(String::as_str),
            Target::Live { .. } => None,
        }
    }
}

#[derive(Default)]
enum Phase {
    #[default]
    Idle,
    /// The script re-runs with the live take disabled, restoring its stops.
    AwaitingLiveReload {
        requested: Instant,
        revision: u64,
    },
    /// The microphone is already open, so the speaker can check the level.
    Countdown {
        target: Target,
        until: Instant,
        input: Box<Input>,
    },
    Recording(Box<Recording>),
}

/// The recording in progress, if any.
#[derive(Resource, Default)]
pub struct NarrationSession {
    phase: Phase,
}

impl NarrationSession {
    /// Whether the recorder owns playback and the keyboard.
    pub fn capturing(&self) -> bool {
        !matches!(self.phase, Phase::Idle)
    }

    /// Stop policy while capturing: a live take pauses at every stop, a
    /// voiceover plays straight through.
    pub fn stop_policy(&self) -> Option<PlaybackStopPolicy> {
        let live = match &self.phase {
            Phase::Idle => return None,
            Phase::AwaitingLiveReload { .. } => true,
            Phase::Countdown { target, .. } => target.is_live(),
            Phase::Recording(recording) => recording.target.is_live(),
        };
        Some(capture_stop_policy(live))
    }
}

fn capture_stop_policy(live: bool) -> PlaybackStopPolicy {
    if live {
        PlaybackStopPolicy::Respect
    } else {
        PlaybackStopPolicy::Ignore
    }
}

fn manifest(stash: &StashedReplay) -> NarrationManifest {
    stash
        .canvas
        .as_ref()
        .map(|canvas| canvas.narration_manifest())
        .unwrap_or_default()
}

/// The script the scene reads, or where the default one goes.
fn script_path(stash: &StashedReplay, manifest: &NarrationManifest) -> Option<PathBuf> {
    manifest
        .script
        .as_ref()
        .map(|script| script.path.clone())
        .or_else(|| {
            let canvas = stash.canvas.as_ref()?;
            Some(canvas.narration_directory().join(DEFAULT_SCRIPT))
        })
}

/// Create the script, or add a section for every voiceover it lacks, and
/// return how many sections were added. New sections start with the text the
/// code or the segment notes gave the voiceover.
fn complete_script(path: &Path, manifest: &NarrationManifest) -> Result<usize, String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let known = match &existing {
        Some(source) => parse_script(source)?
            .into_iter()
            .map(|section| section.key)
            .collect(),
        None => Vec::new(),
    };
    let missing: Vec<(String, String)> = manifest
        .voiceovers
        .iter()
        .filter(|voice| !known.contains(&voice.key))
        .map(|voice| {
            let text = match voice.text_source {
                TextSource::Argument | TextSource::Notes => voice.text.clone().unwrap_or_default(),
                TextSource::Script | TextSource::Missing => String::new(),
            };
            (voice.key.clone(), text)
        })
        .collect();
    let source = match existing {
        None => format_script(&missing),
        Some(mut source) => {
            if missing.is_empty() {
                return Ok(0);
            }
            let added = format_script(&missing);
            // Keep only the new sections of the formatted script.
            let sections = added.find("\n## ").map_or("", |start| &added[start..]);
            if !source.ends_with('\n') {
                source.push('\n');
            }
            source.push_str(sections);
            source
        }
    };
    write_atomically(path, source.as_bytes()).map_err(|error| error.to_string())?;
    Ok(missing.len())
}

/// Seconds a live take spent at a stop: the take time at which playback
/// resumes, minus the scene time that played and the holds already taken.
/// Measured on the recording clock, so the saved audio and the retimed
/// timeline agree at every stop.
fn live_hold(elapsed: f64, stop_time: f64, start_time: f64, previous_holds: &[f64]) -> f64 {
    (elapsed - (stop_time - start_time) - previous_holds.iter().sum::<f64>()).max(0.0)
}

/// Drives requests from the panel, the countdown, and the recording itself.
#[allow(clippy::too_many_arguments)]
pub(crate) fn narration_session_system(
    keys: Res<ButtonInput<KeyCode>>,
    mut session: ResMut<NarrationSession>,
    mut panel: ResMut<NarrationPanel>,
    mut timeline: ResMut<Timeline>,
    mut editor: ResMut<EditorState>,
    mut preview_audio: ResMut<gaanim_media::PreviewAudioEnabled>,
    stash: Res<StashedReplay>,
    reload: Option<Res<ScriptReload>>,
    presentation: Res<PresentationMode>,
) {
    panel.poll_jobs();
    let pressed = |key| keys.just_pressed(key);
    let reload = reload.map(|reload| reload.0.clone());

    let phase = std::mem::take(&mut session.phase);
    session.phase = match phase {
        Phase::Idle => {
            let Some(request) = panel.request.take() else {
                return;
            };
            if presentation.active {
                return;
            }
            let manifest = manifest(&stash);
            match request {
                Request::Listen(time) => {
                    timeline.seek_request = Some(time);
                    timeline.is_playing = true;
                    Phase::Idle
                }
                Request::Transcribe(key) => {
                    if let Some(files) = take_files(&manifest, &key) {
                        match files.audio {
                            Some(take) => panel.transcribe(&key, take, files.sidecar),
                            None => panel.notify(
                                NoticeKind::Error,
                                format!("«{key}» todavía no tiene toma"),
                            ),
                        }
                    }
                    Phase::Idle
                }
                Request::Level(key) => {
                    if let Some(files) = take_files(&manifest, &key) {
                        panel.level_existing(&key, &files);
                    }
                    Phase::Idle
                }
                Request::OpenScript => {
                    match script_path(&stash, &manifest) {
                        Some(path) => match complete_script(&path, &manifest) {
                            Ok(added) => {
                                let opened = open::that(&path);
                                let mut message = match added {
                                    0 => format!("Guion: {}", path.display()),
                                    added => format!(
                                        "Guion {} · {added} bloque(s) nuevo(s)",
                                        path.display()
                                    ),
                                };
                                if manifest
                                    .voiceovers
                                    .iter()
                                    .any(|voice| voice.text_source == TextSource::Argument)
                                {
                                    message.push_str(
                                        ". Quita text=… del código para usar el texto del guion.",
                                    );
                                }
                                if let Err(error) = opened {
                                    message =
                                        format!("{message}. No se pudo abrir el editor: {error}");
                                }
                                panel.notify(NoticeKind::Info, message);
                            }
                            Err(error) => panel.notify(
                                NoticeKind::Error,
                                format!("no se pudo preparar el guion: {error}"),
                            ),
                        },
                        None => panel.notify(NoticeKind::Error, "la escena aún no se cargó"),
                    }
                    Phase::Idle
                }
                Request::RecordVoiceover(key) => {
                    match manifest.voiceovers.iter().find(|voice| voice.key == key) {
                        Some(_) if panel.busy(&key) => {
                            panel.notify(
                                NoticeKind::Info,
                                format!("espera a que termine el trabajo en curso de «{key}»"),
                            );
                            Phase::Idle
                        }
                        Some(spec) => {
                            let target = Target::voiceover(spec);
                            prepare_capture(
                                &target,
                                &mut timeline,
                                &mut editor,
                                &mut preview_audio,
                            );
                            start_countdown(
                                target,
                                &mut panel,
                                &mut timeline,
                                &mut preview_audio,
                                reload.as_deref(),
                            )
                        }
                        None => {
                            panel.notify(
                                NoticeKind::Error,
                                format!("«{key}» ya no está en la escena"),
                            );
                            Phase::Idle
                        }
                    }
                }
                Request::RecordLive => match (Target::live(&manifest), &manifest.live) {
                    (Some(target), Some(live)) => {
                        if panel.busy(target.key()) {
                            panel.notify(
                                NoticeKind::Info,
                                format!(
                                    "espera a que termine el trabajo en curso de «{}»",
                                    target.key()
                                ),
                            );
                            return;
                        }
                        set_live_take_recording(true);
                        prepare_capture(&target, &mut timeline, &mut editor, &mut preview_audio);
                        if !live.applied {
                            // Its stops are already interactive.
                            start_countdown(
                                target,
                                &mut panel,
                                &mut timeline,
                                &mut preview_audio,
                                reload.as_deref(),
                            )
                        } else if let Some(reload) = &reload {
                            reload();
                            Phase::AwaitingLiveReload {
                                requested: Instant::now(),
                                revision: stash.revision,
                            }
                        } else {
                            set_live_take_recording(false);
                            preview_audio.0 = true;
                            panel.notify(NoticeKind::Error, "no se puede recargar la escena");
                            Phase::Idle
                        }
                    }
                    _ => Phase::Idle,
                },
            }
        }
        Phase::AwaitingLiveReload {
            requested,
            revision,
        } => {
            let reloaded = (stash.revision != revision)
                .then(|| manifest(&stash))
                .filter(|manifest| manifest.live.as_ref().is_some_and(|live| live.recording));
            if pressed(KeyCode::Escape) {
                abort_live(&mut panel, reload.as_deref(), None);
                preview_audio.0 = true;
                Phase::Idle
            } else if let Some(target) = reloaded.as_ref().and_then(Target::live) {
                timeline.is_playing = false;
                timeline.seek_request = Some(target.start_time());
                start_countdown(
                    target,
                    &mut panel,
                    &mut timeline,
                    &mut preview_audio,
                    reload.as_deref(),
                )
            } else if requested.elapsed() > LIVE_RELOAD_TIMEOUT {
                abort_live(
                    &mut panel,
                    reload.as_deref(),
                    Some("la escena no se recargó a tiempo; revisa errores del script"),
                );
                preview_audio.0 = true;
                Phase::Idle
            } else {
                Phase::AwaitingLiveReload {
                    requested,
                    revision,
                }
            }
        }
        Phase::Countdown {
            target,
            until,
            mut input,
        } => {
            input.update(Instant::now());
            if pressed(KeyCode::Escape) {
                drop(input);
                finish_capture(&target, &mut timeline, &mut preview_audio);
                if target.is_live() {
                    abort_live(&mut panel, reload.as_deref(), None);
                }
                Phase::Idle
            } else if Instant::now() >= until {
                begin_recording(target, *input, &mut timeline)
            } else {
                Phase::Countdown {
                    target,
                    until,
                    input,
                }
            }
        }
        Phase::Recording(mut recording) => {
            recording.input.update(Instant::now());
            let elapsed = recording.input.recorder.elapsed();
            if let Some(error) = recording.input.recorder.error() {
                let target = recording.target.clone();
                drop(recording);
                finish_capture(&target, &mut timeline, &mut preview_audio);
                if target.is_live() {
                    abort_live(&mut panel, reload.as_deref(), None);
                }
                panel.notify(NoticeKind::Error, format!("el micrófono falló: {error}"));
                return;
            }
            if pressed(KeyCode::Escape) {
                let target = recording.target.clone();
                drop(recording);
                finish_capture(&target, &mut timeline, &mut preview_audio);
                if target.is_live() {
                    abort_live(&mut panel, reload.as_deref(), None);
                }
                panel.notify(NoticeKind::Info, "Grabación descartada");
                return;
            }
            if recording.target.is_live()
                && recording.waiting_at.is_none()
                && !timeline.is_playing
                && timeline.seek_request.is_none()
                && timeline.current_time < timeline.cached_duration - 1e-4
            {
                recording.waiting_at = Some(timeline.current_time);
            }
            if pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
                if let Some(stop_time) = recording.waiting_at.take() {
                    let hold = live_hold(
                        elapsed,
                        stop_time,
                        recording.target.start_time(),
                        &recording.holds,
                    );
                    recording.holds.push(hold);
                }
                save_recording(*recording, &mut timeline, &mut panel, &mut preview_audio);
                return;
            }
            let advance = pressed(KeyCode::Space)
                || pressed(KeyCode::ArrowRight)
                || pressed(KeyCode::PageDown);
            match &recording.target {
                Target::Voiceover { .. } => {
                    if pressed(KeyCode::Space)
                        && let Some(marker) = recording.next_marker().map(str::to_owned)
                    {
                        recording.taps.push((marker, elapsed));
                    }
                }
                Target::Live { start_time, .. } => {
                    if advance && let Some(stop_time) = recording.waiting_at.take() {
                        let hold = live_hold(elapsed, stop_time, *start_time, &recording.holds);
                        recording.holds.push(hold);
                        timeline.is_playing = true;
                    }
                }
            }
            Phase::Recording(recording)
        }
    };
}

fn take_files(manifest: &NarrationManifest, key: &str) -> Option<TakeFiles> {
    manifest
        .voiceovers
        .iter()
        .find(|voice| voice.key == key)
        .map(|voice| voice.files.clone())
        .or_else(|| {
            manifest
                .live
                .as_ref()
                .filter(|live| live.key == key)
                .map(|live| live.files.clone())
        })
}

/// Open the microphone and count down; the countdown doubles as a level
/// check and is not part of the take.
fn start_countdown(
    target: Target,
    panel: &mut NarrationPanel,
    timeline: &mut Timeline,
    preview_audio: &mut gaanim_media::PreviewAudioEnabled,
    reload: Option<&(dyn Fn() + Send + Sync)>,
) -> Phase {
    let recorder = match MicRecorder::start() {
        Ok(recorder) => recorder,
        Err(error) => {
            finish_capture(&target, timeline, preview_audio);
            if target.is_live() {
                abort_live(panel, reload, None);
            }
            panel.notify(NoticeKind::Error, error);
            return Phase::Idle;
        }
    };
    let delay = if panel.settings().countdown {
        COUNTDOWN
    } else {
        Duration::ZERO
    };
    panel.notice = None;
    let now = Instant::now();
    Phase::Countdown {
        target,
        until: now + delay,
        input: Box::new(Input {
            recorder,
            level: InputLevel::new(now),
        }),
    }
}

/// Show the first frame of the take, silence preview audio so the previous
/// take does not leak into the microphone, and drop any loop.
fn prepare_capture(
    target: &Target,
    timeline: &mut Timeline,
    editor: &mut EditorState,
    preview_audio: &mut gaanim_media::PreviewAudioEnabled,
) {
    if editor.segment_loop.is_active() {
        editor.segment_loop.deactivate(timeline);
    }
    timeline.loop_range = None;
    timeline.is_playing = false;
    timeline.seek_request = Some(target.start_time());
    preview_audio.0 = false;
}

fn finish_capture(
    target: &Target,
    timeline: &mut Timeline,
    preview_audio: &mut gaanim_media::PreviewAudioEnabled,
) {
    timeline.is_playing = false;
    timeline.seek_request = Some(target.start_time());
    preview_audio.0 = true;
}

/// Leave live recording and restore the recorded take's timing, if any.
fn abort_live(
    panel: &mut NarrationPanel,
    reload: Option<&(dyn Fn() + Send + Sync)>,
    error: Option<&str>,
) {
    set_live_take_recording(false);
    if let Some(reload) = reload {
        reload();
    }
    if let Some(error) = error {
        panel.notify(NoticeKind::Error, error);
    }
}

fn begin_recording(target: Target, input: Input, timeline: &mut Timeline) -> Phase {
    // The take starts now: drop what the level check captured.
    input.recorder.restart();
    let start = target.start_time();
    // A live take that starts on a stop waits there first.
    let waiting_at = (target.is_live()
        && timeline
            .segments
            .iter()
            .flat_map(|segment| &segment.stops)
            .any(|stop| (stop.time - start).abs() < 1e-5))
    .then_some(start);
    timeline.seek_request = Some(start);
    timeline.is_playing = waiting_at.is_none();
    Phase::Recording(Box::new(Recording {
        target,
        input,
        taps: Vec::new(),
        holds: Vec::new(),
        waiting_at,
    }))
}

fn save_recording(
    recording: Recording,
    timeline: &mut Timeline,
    panel: &mut NarrationPanel,
    preview_audio: &mut gaanim_media::PreviewAudioEnabled,
) {
    let Recording {
        target,
        input,
        taps,
        holds,
        ..
    } = recording;
    let clipped = input.recorder.clipped();
    if target.is_live() {
        // The reload triggered by the new files must apply the take.
        set_live_take_recording(false);
    }
    let (samples, sample_rate) = input.recorder.finish();
    finish_capture(&target, timeline, preview_audio);
    let seconds = samples.len() as f64 / sample_rate.max(1) as f64;
    let key = target.key().to_owned();
    if seconds < MIN_TAKE_SECONDS {
        panel.notify(
            NoticeKind::Error,
            "La toma es demasiado corta; no se guardó",
        );
        return;
    }
    let files = target.files().clone();
    let sidecar = match &target {
        Target::Voiceover { .. } => TakeSidecar {
            markers: taps.into_iter().collect::<BTreeMap<_, _>>(),
            ..TakeSidecar::default()
        },
        Target::Live { .. } => TakeSidecar {
            holds,
            ..TakeSidecar::default()
        },
    };
    let settings = panel.settings().clone();
    // The recording as captured: the take itself, or the original that
    // leveling reads from.
    let recorded = if settings.level_voice {
        originals_dir(&files).join(format!("{key}.wav"))
    } else {
        files.destination.clone()
    };
    // Sidecar first: the audio file is what makes the take count as recorded.
    let written = replace_sidecar(&files.sidecar, &sidecar).and_then(|()| {
        write_wav_take(&recorded, &samples, sample_rate).map_err(|error| error.to_string())
    });
    if let Err(error) = written {
        panel.notify(
            NoticeKind::Error,
            format!("no se pudo guardar la toma: {error}"),
        );
        return;
    }
    if clipped > 0 {
        panel.notify(
            NoticeKind::Error,
            format!(
                "Toma «{key}» guardada, pero saturó {clipped} vez/veces: conviene repetirla \
                 con menos ganancia"
            ),
        );
    } else {
        panel.notify(
            NoticeKind::Success,
            format!("Toma «{key}» guardada · {}", format_seconds(seconds)),
        );
    }
    if settings.level_voice {
        panel.start_leveling(&key, recorded.clone(), &files, settings.loudness);
    }
    if !target.is_live() && settings.auto_transcribe {
        // Leveling keeps timing, so the original transcribes the same.
        panel.transcribe(&key, recorded, files.sidecar.clone());
    }
}

fn format_seconds(seconds: f64) -> String {
    let tenths = (seconds.max(0.0) * 10.0).round() as u64;
    let minutes = tenths / 600;
    let rest = (tenths % 600) as f64 / 10.0;
    if minutes > 0 {
        format!("{minutes}:{rest:04.1}")
    } else {
        format!("{rest:.1} s")
    }
}

fn source_color(source: MarkerSource) -> egui::Color32 {
    match source {
        MarkerSource::Tapped => palette::ACCENT,
        MarkerSource::Transcript => palette::LOOP,
        MarkerSource::Estimated => palette::STOP,
        MarkerSource::Missing => palette::DANGER,
    }
}

fn source_label(source: MarkerSource) -> &'static str {
    match source {
        MarkerSource::Tapped => "marcada",
        MarkerSource::Transcript => "Whisper",
        MarkerSource::Estimated => "estimada",
        MarkerSource::Missing => "no encontrada",
    }
}

fn text_source_label(source: TextSource) -> &'static str {
    match source {
        TextSource::Argument => "texto en el código",
        TextSource::Script => "texto del guion",
        TextSource::Notes => "texto de las notas del segmento",
        TextSource::Missing => "sin texto",
    }
}

fn notice_color(kind: NoticeKind) -> egui::Color32 {
    match kind {
        NoticeKind::Info => palette::TEXT_MUTED,
        NoticeKind::Success => palette::LOOP,
        NoticeKind::Error => palette::DANGER,
    }
}

/// Six dots showing that a floating window can be dragged.
fn drag_grip(ui: &mut egui::Ui) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
    for row in 0..3 {
        for column in 0..2 {
            let center = egui::pos2(
                rect.min.x + 2.5 + column as f32 * 5.0,
                rect.min.y + 3.0 + row as f32 * 5.0,
            );
            ui.painter().rect_filled(
                egui::Rect::from_center_size(center, egui::Vec2::splat(2.0)),
                0.0,
                palette::TEXT_FAINT,
            );
        }
    }
    response.on_hover_text("Arrastra para mover");
}

/// The narration panel: takes of the scene and recorder settings.
#[allow(clippy::too_many_arguments)]
pub(crate) fn narration_panel_system(
    mut contexts: bevy_egui::EguiContexts,
    mut panel: ResMut<NarrationPanel>,
    session: Res<NarrationSession>,
    stash: Res<StashedReplay>,
    presentation: Res<PresentationMode>,
    hub: Option<Res<crate::project_hub::ProjectHubState>>,
) {
    if !panel.open
        || session.capturing()
        || presentation.active
        || hub.is_some_and(|hub| hub.active)
    {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let manifest = manifest(&stash);
    let script = script_path(&stash, &manifest);
    let mut close = false;
    let mut request = None;
    let mut settings_changed = false;
    let screen = ctx.viewport_rect();
    let max_height = (screen.height() - 120.0).max(240.0);

    egui::Area::new(egui::Id::new("narration_panel"))
        .pivot(egui::Align2::RIGHT_TOP)
        .default_pos(egui::pos2(screen.max.x - 12.0, screen.min.y + 12.0))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            card_frame()
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width(360.0);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        drag_grip(ui);
                        paint_mic_badge(ui);
                        ui.label(
                            egui::RichText::new("Narración")
                                .size(15.0)
                                .strong()
                                .color(palette::TEXT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                                .on_hover_text("Cerrar")
                                .clicked()
                            {
                                close = true;
                            }
                        });
                    });

                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if manifest.is_empty() {
                                empty_state(ui);
                            }
                            script_card(ui, &manifest, script.as_deref(), &mut request);
                            if let Some(live) = &manifest.live {
                                ui.label(caption("TOMA EN VIVO"));
                                live_card(ui, live, panel.busy(&live.key), &mut request);
                            }
                            if !manifest.voiceovers.is_empty() {
                                ui.label(caption("VOZ EN OFF"));
                                for voice in &manifest.voiceovers {
                                    voiceover_card(
                                        ui,
                                        voice,
                                        panel.transcribing(&voice.key),
                                        panel.leveling(&voice.key),
                                        &mut request,
                                    );
                                }
                            }
                            ui.add_space(4.0);
                            settings_changed = settings_form(ui, panel.settings());
                        });

                    if let Some((message, kind)) = &panel.notice {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(message)
                                    .size(12.0)
                                    .color(notice_color(*kind)),
                            )
                            .wrap(),
                        );
                    }
                });
        });

    if settings_changed && let Err(error) = panel.settings().save() {
        panel.notify(
            NoticeKind::Error,
            format!("no se guardó la configuración: {error}"),
        );
    }
    if close {
        panel.open = false;
    }
    if let Some(request) = request {
        panel.request = Some(request);
    }
}

fn paint_mic_badge(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(26.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect, 0.0, palette::DANGER.gamma_multiply(0.16));
    paint_icon(
        ui.painter(),
        egui::Rect::from_center_size(rect.center(), egui::Vec2::splat(14.0)),
        Icon::Mic,
        palette::DANGER,
    );
}

fn empty_state(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(
            "La escena no declara narración. Añade un bloque de voz en off \
             y guarda el script:",
        )
        .size(12.5)
        .color(palette::TEXT_MUTED),
    );
    field_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(
                "with scene.voiceover(\"intro\") as vo:\n    \
                 scene.play([titulo.animate.write()])\n    vo.wait_until(\"derivada\")",
            )
            .monospace()
            .size(11.5)
            .color(palette::TEXT),
        );
    });
    ui.label(
        egui::RichText::new(
            "El texto de cada bloque va en el guion (## intro). O graba presentando: \
             llama a scene.live_take(\"clase\") al inicio y usa scene.stop() donde \
             quieras hacer pausas.",
        )
        .size(12.5)
        .color(palette::TEXT_MUTED),
    );
}

/// Where the voiceover texts live, with a button to write them.
fn script_card(
    ui: &mut egui::Ui,
    manifest: &NarrationManifest,
    path: Option<&Path>,
    request: &mut Option<Request>,
) {
    field_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Guion")
                    .size(13.0)
                    .strong()
                    .color(palette::TEXT),
            );
            let detail = match (&manifest.script, path) {
                (Some(script), _) => format!(
                    "{} · {} bloques",
                    file_name(&script.path),
                    script.sections.len()
                ),
                (None, Some(path)) => format!("sin crear · {}", file_name(path)),
                (None, None) => "sin crear".to_string(),
            };
            ui.label(
                egui::RichText::new(detail)
                    .size(12.0)
                    .color(palette::TEXT_MUTED),
            )
            .on_hover_text(path.map_or_else(String::new, |path| path.display().to_string()));
        });
        let missing = manifest
            .voiceovers
            .iter()
            .filter(|voice| {
                manifest
                    .script
                    .as_ref()
                    .is_none_or(|script| script.text(&voice.key).is_none())
            })
            .count();
        let label = match (&manifest.script, missing) {
            (None, _) => "Crear guion".to_string(),
            (Some(_), 0) => "Abrir guion".to_string(),
            (Some(_), missing) => format!("Abrir y añadir {missing} bloque(s)"),
        };
        if secondary_button(ui, &label, path.is_some())
            .on_hover_text(
                "Escribe el texto de cada bloque bajo \"## clave\" en tu editor; \
                 al guardar, la escena se actualiza",
            )
            .clicked()
        {
            *request = Some(Request::OpenScript);
        }
    });
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn status_line(ui: &mut egui::Ui, recorded: bool, detail: String) {
    ui.horizontal(|ui| {
        let color = if recorded {
            palette::LOOP
        } else {
            palette::STOP
        };
        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(8.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 0.0, color);
        ui.label(
            egui::RichText::new(detail)
                .size(12.0)
                .color(palette::TEXT_MUTED),
        );
    });
}

/// Final voice loudness a take was leveled to, or a note that it was not.
fn loudness_detail(loudness: Option<f64>) -> String {
    match loudness {
        Some(loudness) => format!(" · {loudness:.0} LUFS"),
        None => " · sin nivelar".to_string(),
    }
}

/// Final loudness far from the recommendation, and why.
fn loudness_warning(loudness: f64) -> Option<(egui::Color32, &'static str)> {
    if loudness > settings::LOUDNESS_RANGE.1 {
        Some((
            palette::DANGER,
            "Más fuerte que el nivel de YouTube (-14 LUFS): la plataforma la bajará y \
             el limitador de picos trabaja más.",
        ))
    } else if loudness < settings::LOUDNESS_RANGE.0 {
        Some((
            palette::STOP,
            "Por debajo de -20 LUFS la voz sonará débil al lado de otros videos.",
        ))
    } else {
        None
    }
}

fn live_card(
    ui: &mut egui::Ui,
    live: &gaanim_api::canvas::LiveTakeSpec,
    busy: bool,
    request: &mut Option<Request>,
) {
    field_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(&live.key)
                .size(14.0)
                .strong()
                .color(palette::TEXT),
        );
        let stops = match live.stops {
            1 => "1 parada".to_string(),
            count => format!("{count} paradas"),
        };
        let detail = match live.duration {
            Some(duration) if live.applied => format!(
                "Grabada · {} · {stops}{}",
                format_seconds(duration),
                loudness_detail(live.loudness)
            ),
            _ => format!("Sin grabar · {stops} en la escena"),
        };
        status_line(ui, live.applied, detail);
        ui.label(
            egui::RichText::new(
                "Presenta la escena hablando: → o Espacio sigue en cada parada, \
                 Enter termina. El teleprompter muestra la sección del guion con \
                 el nombre del segmento, o sus notas.",
            )
            .size(11.5)
            .color(palette::TEXT_FAINT),
        );
        ui.horizontal_wrapped(|ui| {
            let label = if live.applied {
                "Regrabar toma"
            } else {
                "Grabar toma en vivo"
            };
            if primary_button(ui, label, Some(Icon::Record), !busy).clicked() {
                *request = Some(Request::RecordLive);
            }
            if live.applied {
                if secondary_button(ui, "Oír", true).clicked() {
                    *request = Some(Request::Listen(live.start_time));
                }
                if small_button(ui, if busy { "Nivelando…" } else { "Nivelar" }, !busy)
                    .on_hover_text("Lleva la voz al nivel configurado para video")
                    .clicked()
                {
                    *request = Some(Request::Level(live.key.clone()));
                }
            }
        });
    });
}

fn voiceover_card(
    ui: &mut egui::Ui,
    voice: &VoiceoverSpec,
    transcribing: bool,
    leveling: bool,
    request: &mut Option<Request>,
) {
    let busy = transcribing || leveling;
    field_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&voice.key)
                    .size(14.0)
                    .strong()
                    .color(palette::TEXT),
            );
            if voice.segment != "_default" {
                ui.label(
                    egui::RichText::new(&voice.segment)
                        .size(12.0)
                        .color(palette::TEXT_FAINT),
                );
            }
        });
        let detail = if voice.recorded {
            let transcript = if voice.transcribed {
                " · con transcripción"
            } else {
                ""
            };
            format!(
                "Grabada · {}{}{transcript}",
                format_seconds(voice.duration),
                loudness_detail(voice.loudness)
            )
        } else {
            format!("Sin grabar · ~{} estimados", format_seconds(voice.duration))
        };
        status_line(ui, voice.recorded, detail);
        let text_color = if voice.text_source == TextSource::Missing {
            palette::STOP
        } else {
            palette::TEXT_FAINT
        };
        ui.label(
            egui::RichText::new(text_source_label(voice.text_source))
                .size(11.5)
                .color(text_color),
        );
        if !voice.markers.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                for marker in &voice.markers {
                    let color = source_color(marker.source);
                    let text = egui::RichText::new(&marker.name).size(11.5).color(color);
                    let hover = match marker.offset {
                        Some(offset) => format!(
                            "{} · {}",
                            source_label(marker.source),
                            format_seconds(offset)
                        ),
                        None => source_label(marker.source).to_string(),
                    };
                    egui::Frame::new()
                        .fill(color.gamma_multiply(0.12))
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| ui.label(text))
                        .response
                        .on_hover_text(hover);
                }
            });
        }
        ui.horizontal_wrapped(|ui| {
            let label = if voice.recorded { "Regrabar" } else { "Grabar" };
            if primary_button(ui, label, Some(Icon::Record), !busy).clicked() {
                *request = Some(Request::RecordVoiceover(voice.key.clone()));
            }
            if secondary_button(ui, "Oír", true).clicked() {
                *request = Some(Request::Listen(voice.start_time));
            }
            if voice.recorded {
                let label = if transcribing {
                    "Transcribiendo…"
                } else {
                    "Transcribir"
                };
                if small_button(ui, label, !busy)
                    .on_hover_text("Detecta las marcas por palabra con Whisper")
                    .clicked()
                {
                    *request = Some(Request::Transcribe(voice.key.clone()));
                }
                let label = if leveling { "Nivelando…" } else { "Nivelar" };
                if small_button(ui, label, !busy)
                    .on_hover_text("Lleva la voz al nivel configurado para video")
                    .clicked()
                {
                    *request = Some(Request::Level(voice.key.clone()));
                }
            }
        });
    });
}

/// Recorder, voice level and Whisper preferences. Returns whether they changed.
fn settings_form(ui: &mut egui::Ui, settings: &mut NarrationSettings) -> bool {
    let before = settings.clone();
    egui::CollapsingHeader::new(
        egui::RichText::new("Voz y grabación")
            .size(12.5)
            .color(palette::TEXT_MUTED),
    )
    .id_salt("narration_voice_settings")
    .show(ui, |ui| {
        ui.checkbox(
            &mut settings.level_voice,
            "Nivelar cada toma nueva al guardarla",
        );
        ui.horizontal(|ui| {
            section_label(ui, "Nivel final de la voz");
            ui.add_enabled(
                settings.level_voice,
                egui::DragValue::new(&mut settings.loudness)
                    .range(-24.0..=-10.0)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .suffix(" LUFS"),
            );
            if (settings.loudness - settings::DEFAULT_LOUDNESS).abs() > 0.05
                && small_button(ui, "Recomendado", settings.level_voice)
                    .on_hover_text("Volver a -16 LUFS")
                    .clicked()
            {
                settings.loudness = settings::DEFAULT_LOUDNESS;
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "Recomendado: {:.0} LUFS, voz clara en video online. YouTube reproduce \
                 a -14 y la televisión usa -23. Pico máximo -1.5 dBTP; el original se \
                 conserva en narration/.originals.",
                settings::DEFAULT_LOUDNESS
            ))
            .size(11.5)
            .color(palette::TEXT_FAINT),
        );
        if settings.level_voice
            && let Some((color, warning)) = loudness_warning(settings.loudness)
        {
            ui.label(egui::RichText::new(warning).size(11.5).color(color));
        }
        ui.label(
            egui::RichText::new(format!(
                "Al grabar, los picos deben quedar entre {:.0} y {:.0} dBFS; el medidor \
                 avisa en rojo si te acercas a 0 dBFS.",
                IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1
            ))
            .size(11.5)
            .color(palette::TEXT_FAINT),
        );
        ui.checkbox(
            &mut settings.countdown,
            "Cuenta atrás de 3 s antes de grabar",
        );
    });
    egui::CollapsingHeader::new(
        egui::RichText::new("Whisper (opcional)")
            .size(12.5)
            .color(palette::TEXT_MUTED),
    )
    .id_salt("narration_whisper_settings")
    .show(ui, |ui| {
        ui.label(
            egui::RichText::new(
                "whisper.cpp detecta las marcas por palabra para que no tengas que \
                 pulsar Espacio al grabar. La marca debe llamarse como la palabra \
                 que dices.",
            )
            .size(11.5)
            .color(palette::TEXT_FAINT),
        );
        section_label(ui, "whisper-cli (vacío: buscar en el PATH)");
        path_field(ui, &mut settings.whisper_cli, "whisper-cli", &["exe"]);
        section_label(ui, "Modelo ggml (p. ej. ggml-base.bin)");
        path_field(ui, &mut settings.whisper_model, "Modelo", &["bin"]);
        ui.horizontal(|ui| {
            section_label(ui, "Idioma");
            ui.add(
                egui::TextEdit::singleline(&mut settings.language)
                    .desired_width(60.0)
                    .font(egui::FontId::monospace(12.0)),
            );
        });
        ui.checkbox(
            &mut settings.auto_transcribe,
            "Transcribir cada toma nueva al guardarla",
        );
        let status = match settings.whisper() {
            Ok((cli, _)) => (palette::LOOP, format!("Listo · {}", cli.display())),
            Err(error) => (palette::TEXT_FAINT, error),
        };
        ui.add(egui::Label::new(egui::RichText::new(status.1).size(11.5).color(status.0)).wrap());
    });
    *settings != before
}

fn path_field(ui: &mut egui::Ui, value: &mut String, title: &str, extensions: &[&str]) {
    ui.horizontal(|ui| {
        let browse_width = 34.0;
        ui.add(
            egui::TextEdit::singleline(value)
                .desired_width((ui.available_width() - browse_width - 8.0).max(80.0))
                .font(egui::FontId::monospace(11.5)),
        );
        if small_button(ui, "…", true)
            .on_hover_text("Buscar")
            .clicked()
            && let Some(path) = rfd::FileDialog::new()
                .set_title(title)
                .add_filter(title, extensions)
                .add_filter("Todos", &["*"])
                .pick_file()
        {
            *value = path.to_string_lossy().into_owned();
        }
    });
}

/// Teleprompter, countdown and recording controls over the scene.
pub(crate) fn narration_overlay_system(
    mut contexts: bevy_egui::EguiContexts,
    session: Res<NarrationSession>,
    timeline: Res<Timeline>,
    mut panel: ResMut<NarrationPanel>,
) {
    if !session.capturing() {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    ctx.request_repaint();
    match &session.phase {
        Phase::Idle => {}
        Phase::AwaitingLiveReload { .. } => {
            centered_card(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Preparando la toma en vivo…")
                        .size(16.0)
                        .color(palette::TEXT),
                );
                ui.label(
                    egui::RichText::new(
                        "La escena se recarga con sus paradas originales. Esc cancela.",
                    )
                    .size(12.0)
                    .color(palette::TEXT_MUTED),
                );
            });
        }
        Phase::Countdown {
            target,
            until,
            input,
        } => {
            let remaining = until
                .saturating_duration_since(Instant::now())
                .as_secs_f64();
            centered_card(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", remaining.ceil().max(1.0) as u64))
                        .size(64.0)
                        .strong()
                        .color(palette::DANGER),
                );
                ui.label(
                    egui::RichText::new(format!("Grabando «{}» en un momento", target.key()))
                        .size(14.0)
                        .color(palette::TEXT),
                );
                ui.label(
                    egui::RichText::new(format!("Micrófono: {}", input.recorder.device()))
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                );
                ui.label(
                    egui::RichText::new("Habla como en la toma para probar tu nivel · Esc cancela")
                        .size(12.0)
                        .color(palette::TEXT_MUTED),
                );
            });
            teleprompter(ctx, target, input, None, &timeline, &mut panel.compact);
        }
        Phase::Recording(recording) => {
            teleprompter(
                ctx,
                &recording.target,
                &recording.input,
                Some(recording),
                &timeline,
                &mut panel.compact,
            );
        }
    }
}

fn centered_card(ctx: &egui::Context, contents: impl FnOnce(&mut egui::Ui)) {
    egui::Area::new(egui::Id::new("narration_center"))
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, -80.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(ctx, |ui| {
            card_frame().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    contents(ui);
                });
            });
        });
}

/// The script to read, with the next marker highlighted, plus the recording
/// clock, input level and keys. It floats over the scene: drag it anywhere,
/// or fold it to its status line.
fn teleprompter(
    ctx: &egui::Context,
    target: &Target,
    input: &Input,
    recording: Option<&Recording>,
    timeline: &Timeline,
    compact: &mut bool,
) {
    let screen = ctx.viewport_rect();
    let width = (screen.width() - 48.0).clamp(280.0, 820.0);
    let warning = input.warning(Instant::now());
    // A red outline tells the speaker to fix the level without reading.
    let outline = match &warning {
        Some(warning) => egui::Stroke::new(2.0, warning.color()),
        None => egui::Stroke::new(1.0, palette::PANEL_STROKE),
    };
    egui::Area::new(egui::Id::new("narration_teleprompter"))
        .pivot(egui::Align2::CENTER_BOTTOM)
        .default_pos(egui::pos2(screen.center().x, screen.max.y - 16.0))
        .movable(true)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(10, 10, 14, 232))
                .inner_margin(egui::Margin::symmetric(18, 12))
                .stroke(outline)
                .show(ui, |ui| {
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    ui.horizontal(|ui| {
                        drag_grip(ui);
                        let (dot, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(12.0), egui::Sense::hover());
                        let recording_now = recording.is_some();
                        let blink = (ui.input(|input| input.time) * 2.0) as i64 % 2 == 0;
                        let color = if recording_now && blink {
                            palette::DANGER
                        } else {
                            palette::DANGER.gamma_multiply(0.35)
                        };
                        ui.painter().circle_filled(dot.center(), 6.0, color);
                        let title = if recording_now {
                            "GRABANDO"
                        } else {
                            "PREPARADO"
                        };
                        ui.label(
                            egui::RichText::new(format!("{title} · {}", target.key()))
                                .size(13.0)
                                .strong()
                                .color(palette::TEXT),
                        );
                        if recording.is_some() {
                            ui.label(
                                egui::RichText::new(format_seconds(input.recorder.elapsed()))
                                    .monospace()
                                    .size(13.0)
                                    .color(palette::TEXT_MUTED),
                            );
                        }
                        level_meter(ui, &input.level);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let label = if *compact {
                                "Mostrar guion"
                            } else {
                                "Ocultar guion"
                            };
                            if small_button(ui, label, true).clicked() {
                                *compact = !*compact;
                            }
                        });
                    });

                    match target {
                        Target::Voiceover { text, markers, .. } => {
                            let tapped = recording.map_or(0, |recording| recording.taps.len());
                            let next = markers.get(tapped).map(String::as_str);
                            if !*compact {
                                match text {
                                    Some(text) => {
                                        ui.add(
                                            egui::Label::new(script_job(
                                                text, markers, next, width,
                                            ))
                                            .wrap(),
                                        );
                                    }
                                    None => {
                                        ui.label(
                                            egui::RichText::new(
                                                "Sin texto: escríbelo en el guion bajo \
                                                 \"## clave\" o pasa text=\"…\".",
                                            )
                                            .size(15.0)
                                            .color(palette::TEXT_MUTED),
                                        );
                                    }
                                }
                            }
                            let status = match next {
                                Some(marker) => format!(
                                    "Siguiente marca: «{marker}» · pulsa Espacio al decirla \
                                     ({tapped}/{})",
                                    markers.len()
                                ),
                                None if markers.is_empty() => {
                                    "Sin marcas: lee el texto y pulsa Enter al terminar".to_string()
                                }
                                None => "Todas las marcas listas · Enter para terminar".to_string(),
                            };
                            ui.label(
                                egui::RichText::new(status)
                                    .size(13.0)
                                    .color(palette::ACCENT),
                            );
                        }
                        Target::Live { script, .. } => {
                            let segment = timeline.segment_position.and_then(|position| {
                                timeline
                                    .segments
                                    .iter()
                                    .find(|segment| segment.id == position.segment_id)
                            });
                            if let Some(segment) = segment
                                && !*compact
                            {
                                ui.label(
                                    egui::RichText::new(&segment.name)
                                        .size(13.0)
                                        .color(palette::TEXT_MUTED),
                                );
                                let text = script
                                    .iter()
                                    .find(|(key, _)| *key == segment.name)
                                    .map(|(_, text)| text.as_str())
                                    .filter(|text| !text.trim().is_empty())
                                    .or(segment.notes.as_deref())
                                    .unwrap_or(
                                        "Sin texto: añade \"## <segmento>\" al guion o notes=\"…\" \
                                         en scene.segment().",
                                    );
                                ui.add(
                                    egui::Label::new(
                                        egui::RichText::new(text).size(24.0).color(palette::TEXT),
                                    )
                                    .wrap(),
                                );
                            }
                            let waiting = recording.and_then(|recording| recording.waiting_at);
                            let status = match (waiting, recording) {
                                (Some(_), Some(recording)) => format!(
                                    "En pausa en la parada {} · → o Espacio para seguir",
                                    recording.holds.len() + 1
                                ),
                                (None, Some(_))
                                    if timeline.current_time >= timeline.cached_duration - 1e-4 =>
                                {
                                    "Fin de la escena · Enter para terminar".to_string()
                                }
                                _ => String::new(),
                            };
                            if !status.is_empty() {
                                ui.label(
                                    egui::RichText::new(status).size(13.0).color(palette::STOP),
                                );
                            }
                        }
                    }
                    match &warning {
                        Some(warning) => {
                            ui.label(
                                egui::RichText::new(warning.message())
                                    .size(14.0)
                                    .strong()
                                    .color(warning.color()),
                            );
                        }
                        None if recording.is_none() => {
                            ui.label(
                                egui::RichText::new(format!(
                                    "Prueba de nivel: los picos deben quedar en la zona verde \
                                     ({:.0} a {:.0} dBFS)",
                                    IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1
                                ))
                                .size(13.0)
                                .color(palette::LOOP),
                            );
                        }
                        None => {}
                    }
                    let hints = if target.is_live() {
                        "→/Espacio: seguir · Enter: terminar · Esc: descartar"
                    } else {
                        "Espacio: marca · Enter: terminar · Esc: descartar"
                    };
                    ui.label(
                        egui::RichText::new(hints)
                            .size(11.5)
                            .color(palette::TEXT_FAINT),
                    );
                });
        });
}

/// Input meter in dBFS with the ideal zone in green, the edge of clipping
/// in amber and red, and the current peak.
fn level_meter(ui: &mut egui::Ui, level: &InputLevel) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(170.0, 10.0), egui::Sense::hover());
    let painter = ui.painter();
    let x = |db: f32| {
        let fraction = ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0);
        rect.min.x + rect.width() * fraction
    };
    let band = |from: f32, to: f32, color: egui::Color32| {
        painter.rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(x(from), rect.min.y),
                egui::pos2(x(to), rect.max.y),
            ),
            0.0,
            color.gamma_multiply(0.22),
        );
    };
    painter.rect_filled(rect, 0.0, egui::Color32::from_white_alpha(16));
    band(IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1, palette::LOOP);
    band(IDEAL_PEAK_DB.1, HOT_PEAK_DB, palette::STOP);
    band(HOT_PEAK_DB, 0.0, palette::DANGER);
    let db = to_db(level.peak);
    let color = peak_color(db);
    painter.rect_filled(
        egui::Rect::from_min_max(rect.min, egui::pos2(x(db), rect.max.y)),
        0.0,
        color,
    );
    for mark in [IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1] {
        painter.line_segment(
            [
                egui::pos2(x(mark), rect.min.y - 2.0),
                egui::pos2(x(mark), rect.max.y + 2.0),
            ],
            egui::Stroke::new(1.0, egui::Color32::from_white_alpha(90)),
        );
    }
    response.on_hover_text(format!(
        "Picos ideales entre {:.0} y {:.0} dBFS (verde). Por encima de {:.0} dBFS una \
         palabra fuerte puede saturar, y la saturación no se corrige después.",
        IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1, HOT_PEAK_DB
    ));
    let label = if db <= METER_FLOOR_DB {
        "-inf dB".to_string()
    } else {
        format!("{db:.0} dB")
    };
    ui.label(
        egui::RichText::new(label)
            .monospace()
            .size(12.0)
            .color(color),
    );
}

/// Script text with marker words colored: the next marker in the accent
/// color, markers already tapped dimmed.
fn script_job(
    text: &str,
    markers: &[String],
    next: Option<&str>,
    width: f32,
) -> egui::text::LayoutJob {
    let marker_words =
        |name: &str| -> Vec<String> { name.split_whitespace().map(normalize_word).collect() };
    let next_words = next.map(marker_words).unwrap_or_default();
    let other_words: Vec<String> = markers
        .iter()
        .filter(|marker| Some(marker.as_str()) != next)
        .flat_map(|marker| marker_words(marker))
        .collect();
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = width;
    let font = egui::FontId::proportional(24.0);
    let mut rest = text;
    while !rest.is_empty() {
        let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (word, after) = rest.split_at(word_end);
        let space_end = after
            .find(|character: char| !character.is_whitespace())
            .unwrap_or(after.len());
        let (space, after) = after.split_at(space_end);
        let normalized = normalize_word(word);
        let color = if !normalized.is_empty() && next_words.contains(&normalized) {
            palette::ACCENT
        } else if !normalized.is_empty() && other_words.contains(&normalized) {
            palette::STOP
        } else {
            palette::TEXT
        };
        job.append(word, 0.0, egui::TextFormat::simple(font.clone(), color));
        if !space.is_empty() {
            job.append(
                space,
                0.0,
                egui::TextFormat::simple(font.clone(), palette::TEXT),
            );
        }
        rest = after;
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_holds_are_measured_on_the_recording_clock() {
        // Take starts at scene 2 s; the first stop is at scene 5 s.
        assert_eq!(live_hold(4.5, 5.0, 2.0, &[]), 1.5);
        // Second stop at 7 s, reached after 1.5 s of hold plus 2 s of scene.
        assert_eq!(live_hold(8.0, 7.0, 2.0, &[1.5]), 1.5);
        // Resuming before the scene caught up never yields a negative hold.
        assert_eq!(live_hold(2.9, 5.0, 2.0, &[]), 0.0);
    }

    #[test]
    fn the_input_meter_warns_about_hot_clipped_and_quiet_voices() {
        let start = Instant::now();
        let mut level = InputLevel::new(start);
        level.update(0.25, start); // -12 dBFS: ideal
        assert_eq!(peak_color(to_db(0.25)), palette::LOOP);
        assert_eq!(level.warning(0, start), None);
        let hot = start + Duration::from_millis(100);
        level.update(0.8, hot); // -1.9 dBFS
        assert_eq!(level.warning(0, hot), Some(LevelWarning::Hot));
        assert_eq!(peak_color(to_db(0.8)), palette::DANGER);
        let later = hot + HOT_WARNING + Duration::from_millis(1);
        assert_eq!(level.warning(2, later), Some(LevelWarning::Clipped(2)));
        assert_eq!(level.warning(0, later), None);
        // Only room noise for longer than the grace period.
        let silent = later + QUIET_AFTER + Duration::from_millis(1);
        level.update(0.005, silent);
        assert_eq!(level.warning(0, silent), Some(LevelWarning::Quiet));
        assert_eq!(peak_color(to_db(0.6)), palette::STOP); // -4.4 dBFS
        assert_eq!(peak_color(to_db(0.05)), palette::TEXT_MUTED); // -26 dBFS
    }

    #[test]
    fn final_loudness_outside_the_recommendation_is_flagged() {
        assert!(loudness_warning(settings::DEFAULT_LOUDNESS).is_none());
        assert_eq!(loudness_warning(-12.0).unwrap().0, palette::DANGER);
        assert_eq!(loudness_warning(-22.0).unwrap().0, palette::STOP);
        assert_eq!(loudness_detail(None), " · sin nivelar");
    }

    #[test]
    fn seconds_are_short_and_readable() {
        assert_eq!(format_seconds(4.26), "4.3 s");
        assert_eq!(format_seconds(75.04), "1:15.0");
    }

    #[test]
    fn teleprompter_highlights_the_next_marker() {
        let markers = vec!["recta".to_string(), "pendiente".to_string()];
        let job = script_job(
            "La recta y su pendiente.",
            &markers,
            Some("pendiente"),
            400.0,
        );
        let color_of = |needle: &str| {
            job.sections
                .iter()
                // Adjacent runs with one format merge, e.g. "La " is one section.
                .find(|section| {
                    job.text[section.byte_range.start.0..section.byte_range.end.0].trim() == needle
                })
                .map(|section| section.format.color)
        };
        assert_eq!(color_of("pendiente."), Some(palette::ACCENT));
        assert_eq!(color_of("recta"), Some(palette::STOP));
        assert_eq!(color_of("La"), Some(palette::TEXT));
        assert_eq!(job.text, "La recta y su pendiente.");
    }

    #[test]
    fn capture_policy_follows_the_take_kind() {
        let mut session = NarrationSession::default();
        assert_eq!(session.stop_policy(), None);
        assert!(!session.capturing());
        session.phase = Phase::AwaitingLiveReload {
            requested: Instant::now(),
            revision: 0,
        };
        assert_eq!(session.stop_policy(), Some(PlaybackStopPolicy::Respect));
        assert!(session.capturing());
        assert_eq!(capture_stop_policy(false), PlaybackStopPolicy::Ignore);
    }

    fn voice(key: &str, text: Option<&str>, source: TextSource) -> VoiceoverSpec {
        VoiceoverSpec {
            key: key.into(),
            text: text.map(str::to_owned),
            text_source: source,
            segment: "_default".into(),
            start_time: 0.0,
            duration: 1.0,
            recorded: false,
            transcribed: false,
            files: TakeFiles::locate(Path::new("narration"), key),
            markers: Vec::new(),
            loudness: None,
        }
    }

    #[test]
    fn opening_the_script_creates_it_then_adds_missing_sections() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("narration").join(DEFAULT_SCRIPT);
        let mut manifest = NarrationManifest {
            voiceovers: vec![
                voice("intro", Some("Hola desde el código"), TextSource::Argument),
                voice("tema", None, TextSource::Missing),
            ],
            ..NarrationManifest::default()
        };
        assert_eq!(complete_script(&path, &manifest).unwrap(), 2);
        let sections = parse_script(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(sections[0].text, "Hola desde el código");
        assert_eq!(sections[1].text, "");

        // The author wrote a text; a new voiceover appears in the code.
        let edited = std::fs::read_to_string(&path)
            .unwrap()
            .replace("## tema\n", "## tema\n\nEl tema de hoy.\n");
        std::fs::write(&path, edited).unwrap();
        manifest
            .voiceovers
            .push(voice("cierre", Some("notas"), TextSource::Notes));
        assert_eq!(complete_script(&path, &manifest).unwrap(), 1);
        let sections = parse_script(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let texts: Vec<(&str, &str)> = sections
            .iter()
            .map(|section| (section.key.as_str(), section.text.as_str()))
            .collect();
        assert_eq!(
            texts,
            [
                ("intro", "Hola desde el código"),
                ("tema", "El tema de hoy."),
                ("cierre", "notas")
            ]
        );
        assert_eq!(complete_script(&path, &manifest).unwrap(), 0);
    }
}
