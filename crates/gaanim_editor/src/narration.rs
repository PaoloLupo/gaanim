//! Narration recorder.
//!
//! Records the voice for a scene without leaving the editor. A voiceover
//! take is recorded while its block plays, with its script as teleprompter;
//! pressing Space when saying a marker stores that instant. A live take is
//! recorded by presenting the scene: playback pauses at each `stop()` and
//! the time spent there becomes a hold that replaces the stop. Takes and
//! their sidecars land in the scene's `narration/` folder, leveled to a
//! standard voice loudness, and the file watcher reloads the scene with the
//! new timing. Texts can live in a Markdown script edited outside the code,
//! which Scribe can dictate while the speaker presents (see [`scribe`]).

mod level;
mod recorder;
// The web cannot open Scribe's socket: its session only reports that.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
mod scribe;
mod settings;
mod whisper;

use bevy::platform::time::Instant;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::egui;
use gaanim_api::canvas::{
    MarkerSource, NarrationManifest, ScriptSection, TakeFiles, TextSource, VoiceoverSpec,
    set_live_take_recording,
};
use gaanim_media::narration::{
    DEFAULT_SCRIPT, TakeSidecar, format_script, normalize_word, parse_script, write_atomically,
    write_wav_take,
};
use gaanim_timeline::notes::{mark_line, marks};
use gaanim_timeline::timeline::{PlaybackStopPolicy, Timeline};

use crate::presenter::prompter::{
    IMPLICIT_SEGMENT, PrompterWords, ScriptPosition, script_lines, show_script,
};
use scribe::{ScribeConfig, ScribeEvent, ScribeSession, ScribeToken, StopReason, TokenSource};

use crate::export::{ProjectPaths, StashedReplay};
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
    /// A Scribe token typed for this session. It is never saved or shown.
    scribe_token: String,
    /// Whether the credential store holds a token, once looked up.
    token_stored: Option<bool>,
    /// What the project's gaanim.toml says, read again on every replay.
    speech: Option<(u64, ProjectSpeech)>,
    /// Dictate the script with Scribe while recording the live take.
    dictate_live: bool,
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
    RecordLive {
        dictate: bool,
    },
    /// Present the scene and dictate its script with Scribe.
    Dictate,
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
        match kind {
            NoticeKind::Info => gaanim_core::console::info("narration", &message),
            NoticeKind::Success => gaanim_core::console::success("narration", &message),
            NoticeKind::Error => gaanim_core::console::error("narration", &message),
        }
        self.notice = Some((message, kind));
    }

    fn transcribing(&self, key: &str) -> bool {
        self.transcriptions.iter().any(|job| job.key == key)
    }

    /// Whether a request waits for the session or a transcription or a
    /// leveling still runs in the background.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn working(&self) -> bool {
        self.request.is_some() || !self.transcriptions.is_empty() || !self.levelings.is_empty()
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
    },
    /// Present the scene to dictate its script; no take is recorded.
    Dictation { start_time: f64 },
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
        })
    }

    fn key(&self) -> &str {
        match self {
            Self::Voiceover { key, .. } | Self::Live { key, .. } => key,
            Self::Dictation { .. } => "guion",
        }
    }

    fn start_time(&self) -> f64 {
        match self {
            Self::Voiceover { start_time, .. }
            | Self::Live { start_time, .. }
            | Self::Dictation { start_time } => *start_time,
        }
    }

    fn files(&self) -> Option<&TakeFiles> {
        match self {
            Self::Voiceover { files, .. } | Self::Live { files, .. } => Some(files),
            Self::Dictation { .. } => None,
        }
    }

    fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }

    /// Whether the speaker presents the scene, pausing at every stop.
    fn presents(&self) -> bool {
        matches!(self, Self::Live { .. } | Self::Dictation { .. })
    }
}

/// What a capture does besides recording: dictate the script with Scribe,
/// and whether the take is kept.
struct CapturePlan {
    scribe: Option<(ScribeToken, ScribeConfig)>,
    keep_take: bool,
}

impl CapturePlan {
    const TAKE: Self = Self {
        scribe: None,
        keep_take: true,
    };
}

/// The script Scribe dictates during a capture.
struct Dictation {
    session: ScribeSession,
    /// Recording seconds at which the slide part being said changed, with
    /// the slide (by name) and part from then on.
    positions: Vec<(f64, Option<(String, usize)>)>,
    /// Transcribed turns: when they started, in recording seconds, and
    /// their text.
    turns: Vec<(f64, String)>,
    ready: bool,
    speaking: bool,
    /// A warning from Scribe, or why the session stopped or failed.
    note: Option<(String, bool)>,
    closed: bool,
}

impl Dictation {
    fn new(session: ScribeSession) -> Self {
        Self {
            session,
            positions: Vec::new(),
            turns: Vec::new(),
            ready: false,
            speaking: false,
            note: None,
            closed: false,
        }
    }

    /// Take in what the session reported.
    fn absorb(&mut self) {
        for event in self.session.poll() {
            match event {
                ScribeEvent::Ready => self.ready = true,
                ScribeEvent::Speech(speaking) => self.speaking = speaking,
                ScribeEvent::Turn { start, text, .. } => {
                    self.speaking = false;
                    self.turns.push((start, text));
                }
                ScribeEvent::Warning(message) => self.note = Some((message, false)),
                ScribeEvent::Stopping(reason) => {
                    self.note = Some((
                        match reason {
                            StopReason::Limit => {
                                "Scribe se detuvo: se alcanzó el límite de minutos".to_owned()
                            }
                            StopReason::Silence => {
                                "Scribe se detuvo tras dos minutos sin voz".to_owned()
                            }
                            StopReason::NoAudio => {
                                "Scribe se detuvo: el micrófono dejó de enviar audio".to_owned()
                            }
                        },
                        false,
                    ));
                }
                ScribeEvent::Failed(message) => self.note = Some((message, true)),
                ScribeEvent::Closed => {
                    self.closed = true;
                    self.speaking = false;
                }
            }
        }
    }

    /// Note where the speaker is in the script, `elapsed` seconds into the
    /// recording.
    fn track(&mut self, timeline: &Timeline, elapsed: f64) {
        let position = ScriptPosition::of(timeline).map(|position| {
            (
                timeline.segments[position.slide].name.clone(),
                position.part,
            )
        });
        if self.positions.last().map(|(_, last)| last) != Some(&position) {
            self.positions.push((elapsed, position));
        }
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
    /// The script Scribe dictates meanwhile.
    dictation: Option<Dictation>,
    /// Save the take when the speaker finishes.
    keep_take: bool,
}

impl Recording {
    fn next_marker(&self) -> Option<&str> {
        match &self.target {
            Target::Voiceover { markers, .. } => markers.get(self.taps.len()).map(String::as_str),
            Target::Live { .. } | Target::Dictation { .. } => None,
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
        plan: CapturePlan,
    },
    /// The microphone is already open, so the speaker can check the level.
    Countdown {
        target: Target,
        until: Instant,
        input: Box<Input>,
        plan: CapturePlan,
    },
    Recording(Box<Recording>),
    /// The capture ended; Scribe sends its last turns before they go into
    /// the script.
    Finishing {
        dictation: Box<Dictation>,
        since: Instant,
    },
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
            Phase::AwaitingLiveReload { .. } | Phase::Finishing { .. } => true,
            Phase::Countdown { target, .. } => target.presents(),
            Phase::Recording(recording) => recording.target.presents(),
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

/// A slide as the script sees it.
#[derive(Debug, Clone, PartialEq)]
struct ScriptSlide {
    name: String,
    /// The notes it shows: its section's, else its `notes=`.
    notes: Option<String>,
    /// Its stops' names.
    stops: Vec<Option<String>>,
}

/// The authored slides of the scene, in order.
fn script_slides(stash: &StashedReplay) -> Vec<ScriptSlide> {
    let Some(canvas) = stash.canvas.as_ref() else {
        return Vec::new();
    };
    canvas
        .segment_manifest()
        .segments
        .into_iter()
        .filter(|segment| segment.name != IMPLICIT_SEGMENT)
        .map(|segment| ScriptSlide {
            name: segment.name,
            notes: segment.notes,
            stops: segment.stops.into_iter().map(|stop| stop.name).collect(),
        })
        .collect()
}

/// What completing the script added to it.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ScriptAdditions {
    sections: usize,
    marks: usize,
    /// New slide sections that start with the slide's `notes=`.
    from_notes: usize,
}

impl ScriptAdditions {
    fn is_empty(self) -> bool {
        self.sections == 0 && self.marks == 0
    }
}

/// The `[stop]` lines `slide` still lacks after the `marked` it has.
fn missing_marks(slide: &ScriptSlide, marked: usize) -> Vec<String> {
    slide
        .stops
        .iter()
        .skip(marked)
        .map(|name| mark_line(name.as_deref()))
        .collect()
}

/// How many things completing the script would add: sections for slides
/// and voiceovers without one, and slides whose section misses marks.
fn script_gaps(
    sections: &[ScriptSection],
    manifest: &NarrationManifest,
    slides: &[ScriptSlide],
) -> usize {
    let slide_gaps = slides
        .iter()
        .filter(|slide| {
            match sections
                .iter()
                .find(|section| section.is_named(&slide.name))
            {
                Some(section) => marks(&section.notes).len() < slide.stops.len(),
                None => true,
            }
        })
        .count();
    let voice_gaps = manifest
        .voiceovers
        .iter()
        .filter(|voice| {
            !sections.iter().any(|section| section.key == voice.key)
                && !slides
                    .iter()
                    .any(|slide| slide.name.to_lowercase() == voice.key.to_lowercase())
        })
        .count();
    slide_gaps + voice_gaps
}

/// The script `existing` completed: a section for every slide and voiceover
/// it lacks, and a `[stop]` mark for every stop a slide's section does not
/// mark yet. A new slide section starts with the slide's `notes=` and goes
/// after the section of the slide before it; a new voiceover section
/// starts with the text the code or the notes gave the take and goes at the
/// end. What is written is never removed or rewritten.
fn complete_source(
    existing: Option<&str>,
    manifest: &NarrationManifest,
    slides: &[ScriptSlide],
) -> Result<(String, ScriptAdditions), String> {
    let sections = match existing {
        Some(source) => parse_script(source)?,
        None => Vec::new(),
    };
    let mut additions = ScriptAdditions::default();
    // Lines to insert after a 1-based line of `existing` (0: before the
    // first), in order.
    let mut insertions: Vec<(usize, Vec<String>)> = Vec::new();
    // New sections, in order, for a script that does not exist yet.
    let mut fresh: Vec<(String, String)> = Vec::new();
    let line_count = existing.map_or(0, |source| source.lines().count());
    let section_of = |name: &str| sections.iter().find(|section| section.is_named(name));

    for (index, slide) in slides.iter().enumerate() {
        if let Some(section) = section_of(&slide.name) {
            let lacking = missing_marks(slide, marks(&section.notes).len());
            if !lacking.is_empty() {
                additions.marks += lacking.len();
                let mut lines = Vec::new();
                for mark in lacking {
                    lines.push(String::new());
                    lines.push(mark);
                }
                insertions.push((section.end, lines));
            }
            continue;
        }
        let notes = slide.notes.as_deref().map(str::trim).unwrap_or_default();
        let lacking = missing_marks(slide, marks(notes).len());
        additions.sections += 1;
        additions.marks += lacking.len();
        if !notes.is_empty() {
            additions.from_notes += 1;
        }
        let body = std::iter::once(notes.to_owned())
            .chain(lacking)
            .filter(|paragraph| !paragraph.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        // After the section of the closest slide before it that has one,
        // else before the first later slide's section, else at the end.
        let earlier = slides[..index]
            .iter()
            .rev()
            .find_map(|earlier| section_of(&earlier.name))
            .map(|section| section.end);
        let later = slides[index + 1..]
            .iter()
            .find_map(|later| section_of(&later.name))
            .map(|section| section.line - 1);
        let after = earlier.or(later).unwrap_or(line_count);
        let mut lines = vec![String::new(), format!("## {}", slide.name)];
        if !body.is_empty() {
            lines.push(String::new());
            lines.extend(body.lines().map(str::to_owned));
        }
        if earlier.is_none() && later.is_some() {
            // Keep a blank line before the section that follows.
            lines.push(String::new());
        }
        insertions.push((after, lines));
        fresh.push((slide.name.clone(), body));
    }
    for voice in &manifest.voiceovers {
        let covered = sections.iter().any(|section| section.key == voice.key)
            || slides
                .iter()
                .any(|slide| slide.name.to_lowercase() == voice.key.to_lowercase())
            || fresh.iter().any(|(key, _)| *key == voice.key);
        if covered {
            continue;
        }
        let text = match voice.text_source {
            TextSource::Argument | TextSource::Notes => voice.text.clone().unwrap_or_default(),
            TextSource::Script | TextSource::Missing => String::new(),
        };
        additions.sections += 1;
        let mut lines = vec![String::new(), format!("## {}", voice.key)];
        if !text.trim().is_empty() {
            lines.push(String::new());
            lines.extend(text.trim().lines().map(str::to_owned));
        }
        insertions.push((line_count, lines));
        fresh.push((voice.key.clone(), text));
    }

    let Some(existing) = existing else {
        return Ok((format_script(&fresh), additions));
    };
    // Keep the file's line endings.
    let newline = if existing.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output: Vec<String> = Vec::new();
    let insert_after = |line: usize, output: &mut Vec<String>| {
        for (_, lines) in insertions.iter().filter(|(after, _)| *after == line) {
            output.extend(lines.iter().cloned());
        }
    };
    insert_after(0, &mut output);
    for (index, line) in existing.lines().enumerate() {
        output.push(line.to_owned());
        insert_after(index + 1, &mut output);
    }
    let mut source = output.join(newline);
    source.push_str(newline);
    Ok((source, additions))
}

/// Create the script at `path`, or complete it (see [`complete_source`]).
fn complete_script(
    path: &Path,
    manifest: &NarrationManifest,
    slides: &[ScriptSlide],
) -> Result<ScriptAdditions, String> {
    let existing = match std::fs::read_to_string(path) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    let (source, additions) = complete_source(existing.as_deref(), manifest, slides)?;
    if existing.is_none() || !additions.is_empty() {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        write_atomically(path, source.as_bytes()).map_err(|error| error.to_string())?;
    }
    Ok(additions)
}

/// What dictating added to the script.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DictationReport {
    /// Slide parts that received dictated text.
    parts: usize,
    /// Of those, parts that already had text, where it went as a comment.
    commented: usize,
}

/// The script `existing` completed (see [`complete_source`]) with the
/// dictated text of each `(slide, part, text)`: before the part's stop
/// mark, or at the end of the section for the text after the last one. A
/// part that already has text keeps it, and the dictation goes beside it as
/// an HTML comment, which the script ignores.
fn merge_dictation(
    existing: Option<&str>,
    slides: &[ScriptSlide],
    dictated: &[(String, usize, String)],
) -> Result<(String, DictationReport), String> {
    let (completed, _) = complete_source(existing, &NarrationManifest::default(), slides)?;
    let sections = parse_script(&completed)?;
    let mut report = DictationReport::default();
    // Lines to insert before a 1-based line (one past the last: the end).
    let mut insertions: Vec<(usize, Vec<String>)> = Vec::new();
    for (slide, part, text) in dictated {
        let Some(section) = sections.iter().find(|section| section.is_named(slide)) else {
            continue;
        };
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let written = gaanim_timeline::notes::split_notes(&section.notes)
            .get(*part)
            .is_some_and(|written| !written.text.trim().is_empty());
        let paragraph = if written {
            report.commented += 1;
            // A comment ends at the first `-->`.
            format!("<!-- dictado: {} -->", text.replace("--", "–"))
        } else {
            text.to_owned()
        };
        report.parts += 1;
        match section.marks.get(*part) {
            Some(&mark) => insertions.push((mark, vec![paragraph, String::new()])),
            None => insertions.push((section.end + 1, vec![String::new(), paragraph])),
        }
    }
    let newline = if completed.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let lines: Vec<&str> = completed.lines().collect();
    let mut output: Vec<String> = Vec::with_capacity(lines.len() + insertions.len() * 2);
    for line in 1..=lines.len() + 1 {
        for (_, inserted) in insertions.iter().filter(|(before, _)| *before == line) {
            output.extend(inserted.iter().cloned());
        }
        if let Some(text) = lines.get(line - 1) {
            output.push((*text).to_owned());
        }
    }
    let mut source = output.join(newline);
    source.push_str(newline);
    Ok((source, report))
}

/// The plan of a dictation: the token and what to ask Scribe, from the
/// speaker's preferences.
/// What the open project's `gaanim.toml` says about speech.
#[derive(Debug, Clone, Default, PartialEq)]
struct ProjectSpeech {
    /// The manifest, when the scene belongs to a project.
    manifest: Option<PathBuf>,
    /// Its `language`.
    language: Option<String>,
    /// Its keys that look like credentials.
    secret_keys: Vec<String>,
}

impl ProjectSpeech {
    fn read(paths: Option<&ProjectPaths>) -> Self {
        paths
            .and_then(|paths| gaanim_project::resolve_project(&paths.project_dir).ok())
            .map(|project| Self {
                manifest: Some(project.manifest_path),
                language: project.manifest.language,
                secret_keys: project.manifest.secret_keys,
            })
            .unwrap_or_default()
    }

    /// The Scribe locale the project pins, if it pins one Scribe has.
    fn locale(&self) -> Option<&'static str> {
        self.language.as_deref().and_then(scribe::locale_for)
    }
}

/// The locale Scribe dictates in: the project's `language`, else the
/// speaker's choice, else Spanish.
fn dictation_language(project: &ProjectSpeech, settings: &NarrationSettings) -> &'static str {
    project
        .locale()
        .or_else(|| scribe::locale_for(&settings.scribe_language))
        .unwrap_or(scribe::LANGUAGES[0])
}

fn dictation_plan(
    panel: &mut NarrationPanel,
    slides: &[ScriptSlide],
    project: &ProjectSpeech,
    keep_take: bool,
) -> Result<CapturePlan, String> {
    if slides.is_empty() {
        return Err(
            "La escena no tiene diapositivas: crea segmentos con scene.segment(...)".to_owned(),
        );
    }
    let (token, _) = ScribeToken::resolve(&panel.scribe_token, scribe::vault::load, |name| {
        std::env::var(name).ok()
    })
    .ok_or_else(|| {
        "Falta el token de Scribe: guárdalo o pégalo en Ajustes → Scribe de Zoom".to_owned()
    })?;
    let settings = panel.settings();
    let language = dictation_language(project, settings).to_owned();
    let phrases = if settings.scribe_vocabulary {
        vocabulary(slides)
    } else {
        Vec::new()
    };
    let config = ScribeConfig {
        language,
        phrases,
        max_minutes: settings
            .scribe_max_minutes
            .clamp(1, scribe::MAX_SESSION_MINUTES),
    };
    Ok(CapturePlan {
        scribe: Some((token, config)),
        keep_take,
    })
}

/// The slides' and stops' names, once each, as vocabulary for Scribe.
fn vocabulary(slides: &[ScriptSlide]) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    let names = slides.iter().flat_map(|slide| {
        std::iter::once(slide.name.as_str()).chain(slide.stops.iter().flatten().map(String::as_str))
    });
    for name in names {
        let name = name.trim();
        if !name.is_empty() && !words.iter().any(|word| word.eq_ignore_ascii_case(name)) {
            words.push(name.to_owned());
        }
    }
    words.truncate(100);
    words
}

/// Where a dictation starts: the slide under the playhead.
fn dictation_start(timeline: &Timeline) -> f64 {
    ScriptPosition::of(timeline)
        .map(|position| {
            crate::presenter::segment_entry_time(timeline, &timeline.segments[position.slide])
        })
        .unwrap_or(0.0)
}

/// Write what Scribe dictated into the script and say how it went.
fn write_dictation(dictation: Dictation, stash: &StashedReplay, panel: &mut NarrationPanel) {
    let parts = scribe::dictated_parts(&dictation.positions, &dictation.turns);
    let failure = dictation
        .note
        .as_ref()
        .filter(|(_, failed)| *failed)
        .map(|(message, _)| message.clone());
    drop(dictation);
    if parts.is_empty() {
        let message = match failure {
            Some(failure) => format!("Scribe no dictó nada: {failure}"),
            None => "Scribe no transcribió ninguna frase".to_owned(),
        };
        panel.notify(NoticeKind::Error, message);
        return;
    }
    let manifest = manifest(stash);
    let slides = script_slides(stash);
    let Some(path) = script_path(stash, &manifest) else {
        panel.notify(NoticeKind::Error, "la escena aún no se cargó");
        return;
    };
    let existing = match std::fs::read_to_string(&path) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            panel.notify(
                NoticeKind::Error,
                format!("no se pudo leer el guion: {error}"),
            );
            return;
        }
    };
    let written =
        merge_dictation(existing.as_deref(), &slides, &parts).and_then(|(source, report)| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            write_atomically(&path, source.as_bytes())
                .map(|()| report)
                .map_err(|error| error.to_string())
        });
    match written {
        Ok(report) => {
            // Counts only: the dictated words stay in the script.
            let mut message = format!(
                "Guion dictado: {} tramo(s) en {}",
                report.parts,
                file_name(&path)
            );
            if report.commented > 0 {
                message.push_str(&format!(
                    "; {} junto a texto que ya tenías, como comentario",
                    report.commented
                ));
            }
            if let Some(failure) = failure {
                message.push_str(&format!(". Scribe se cortó: {failure}"));
            }
            panel.notify(NoticeKind::Success, message);
        }
        Err(error) => panel.notify(
            NoticeKind::Error,
            format!("no se pudo escribir el guion: {error}"),
        ),
    }
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
    paths: Option<Res<ProjectPaths>>,
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
                    let slides = script_slides(&stash);
                    match script_path(&stash, &manifest) {
                        Some(path) => match complete_script(&path, &manifest, &slides) {
                            Ok(added) => {
                                let opened = crate::platform::open(&path);
                                let mut message = format!("Guion: {}", path.display());
                                let mut news = Vec::new();
                                if added.sections > 0 {
                                    news.push(format!("{} sección(es) nueva(s)", added.sections));
                                }
                                if added.marks > 0 {
                                    news.push(format!("{} marca(s) [stop]", added.marks));
                                }
                                if !news.is_empty() {
                                    message.push_str(&format!(" · {}", news.join(", ")));
                                }
                                if added.from_notes > 0 {
                                    message.push_str(
                                        ". Las notas del código pasaron al guion, que manda sobre \
                                         notes=…: puedes quitarlas del código.",
                                    );
                                }
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
                                CapturePlan::TAKE,
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
                Request::RecordLive { dictate } => {
                    let plan = if dictate {
                        let project = ProjectSpeech::read(paths.as_deref());
                        match dictation_plan(&mut panel, &script_slides(&stash), &project, true) {
                            Ok(plan) => plan,
                            Err(error) => {
                                panel.notify(NoticeKind::Error, error);
                                return;
                            }
                        }
                    } else {
                        CapturePlan::TAKE
                    };
                    match (Target::live(&manifest), &manifest.live) {
                        (Some(target), Some(live)) => start_live(
                            target,
                            live.applied,
                            plan,
                            &mut panel,
                            &mut timeline,
                            &mut editor,
                            &mut preview_audio,
                            reload.as_ref(),
                            stash.revision,
                        ),
                        _ => Phase::Idle,
                    }
                }
                Request::Dictate => {
                    let project = ProjectSpeech::read(paths.as_deref());
                    let plan =
                        match dictation_plan(&mut panel, &script_slides(&stash), &project, false) {
                            Ok(plan) => plan,
                            Err(error) => {
                                panel.notify(NoticeKind::Error, error);
                                return;
                            }
                        };
                    match (Target::live(&manifest), &manifest.live) {
                        // A live take replaces the stops: present with its
                        // stops restored, and keep the take as it was.
                        (Some(target), Some(live)) => start_live(
                            target,
                            live.applied,
                            plan,
                            &mut panel,
                            &mut timeline,
                            &mut editor,
                            &mut preview_audio,
                            reload.as_ref(),
                            stash.revision,
                        ),
                        _ => {
                            let target = Target::Dictation {
                                start_time: dictation_start(&timeline),
                            };
                            prepare_capture(
                                &target,
                                &mut timeline,
                                &mut editor,
                                &mut preview_audio,
                            );
                            start_countdown(
                                target,
                                plan,
                                &mut panel,
                                &mut timeline,
                                &mut preview_audio,
                                reload.as_deref(),
                            )
                        }
                    }
                }
            }
        }
        Phase::AwaitingLiveReload {
            requested,
            revision,
            plan,
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
                    plan,
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
                    plan,
                }
            }
        }
        Phase::Countdown {
            target,
            until,
            mut input,
            plan,
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
                // The take starts now: drop what the level check captured.
                input.recorder.restart();
                let CapturePlan {
                    scribe: dictate,
                    keep_take,
                } = plan;
                let dictation = match dictate {
                    None => None,
                    // Scribe streams from the start of the take, so its
                    // clock is the recording's.
                    Some((token, config)) => {
                        match ScribeSession::start(token, config, input.recorder.tap()) {
                            Ok(session) => Some(Dictation::new(session)),
                            Err(error) if keep_take => {
                                panel.notify(
                                    NoticeKind::Error,
                                    format!("La toma se graba sin dictado: {error}"),
                                );
                                None
                            }
                            Err(error) => {
                                drop(input);
                                finish_capture(&target, &mut timeline, &mut preview_audio);
                                if target.is_live() {
                                    abort_live(&mut panel, reload.as_deref(), None);
                                }
                                panel.notify(NoticeKind::Error, error);
                                return;
                            }
                        }
                    }
                };
                begin_recording(target, *input, &mut timeline, dictation, keep_take)
            } else {
                Phase::Countdown {
                    target,
                    until,
                    input,
                    plan,
                }
            }
        }
        Phase::Finishing {
            mut dictation,
            since,
        } => {
            dictation.absorb();
            // Esc stops waiting; what arrived still goes into the script.
            if dictation.closed || pressed(KeyCode::Escape) || since.elapsed() > FINISH_WAIT {
                write_dictation(*dictation, &stash, &mut panel);
                Phase::Idle
            } else {
                Phase::Finishing { dictation, since }
            }
        }
        Phase::Recording(mut recording) => {
            recording.input.update(Instant::now());
            let elapsed = recording.input.recorder.elapsed();
            if let Some(dictation) = &mut recording.dictation {
                dictation.absorb();
                dictation.track(&timeline, elapsed);
            }
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
                let message = if recording.keep_take {
                    "Grabación descartada"
                } else {
                    "Dictado descartado"
                };
                // Dropping the dictation ends its Scribe session at once.
                drop(recording);
                finish_capture(&target, &mut timeline, &mut preview_audio);
                if target.is_live() {
                    abort_live(&mut panel, reload.as_deref(), None);
                }
                panel.notify(NoticeKind::Info, message);
                return;
            }
            if recording.target.presents()
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
                let dictation = recording.dictation.take();
                if recording.keep_take {
                    save_recording(*recording, &mut timeline, &mut panel, &mut preview_audio);
                } else {
                    let target = recording.target.clone();
                    drop(recording);
                    finish_capture(&target, &mut timeline, &mut preview_audio);
                    if target.is_live() {
                        // Back to the take as it was recorded.
                        abort_live(&mut panel, reload.as_deref(), None);
                    }
                }
                if let Some(dictation) = dictation {
                    dictation.session.finish();
                    session.phase = Phase::Finishing {
                        dictation: Box::new(dictation),
                        since: Instant::now(),
                    };
                }
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
                Target::Live { start_time, .. } | Target::Dictation { start_time } => {
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

/// The longest wait for Scribe's last turns after a dictation.
const FINISH_WAIT: Duration = Duration::from_secs(12);

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
/// Present the scene's live take: at once when its stops are interactive,
/// else after the script re-runs with them restored.
#[allow(clippy::too_many_arguments)]
fn start_live(
    target: Target,
    applied: bool,
    plan: CapturePlan,
    panel: &mut NarrationPanel,
    timeline: &mut Timeline,
    editor: &mut EditorState,
    preview_audio: &mut gaanim_media::PreviewAudioEnabled,
    reload: Option<&Arc<dyn Fn() + Send + Sync>>,
    revision: u64,
) -> Phase {
    if panel.busy(target.key()) {
        panel.notify(
            NoticeKind::Info,
            format!(
                "espera a que termine el trabajo en curso de «{}»",
                target.key()
            ),
        );
        return Phase::Idle;
    }
    set_live_take_recording(true);
    prepare_capture(&target, timeline, editor, preview_audio);
    if !applied {
        // Its stops are already interactive.
        start_countdown(
            target,
            plan,
            panel,
            timeline,
            preview_audio,
            reload.map(|reload| &**reload),
        )
    } else if let Some(reload) = reload {
        reload();
        Phase::AwaitingLiveReload {
            requested: Instant::now(),
            revision,
            plan,
        }
    } else {
        set_live_take_recording(false);
        preview_audio.0 = true;
        panel.notify(NoticeKind::Error, "no se puede recargar la escena");
        Phase::Idle
    }
}

fn start_countdown(
    target: Target,
    plan: CapturePlan,
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
        plan,
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

fn begin_recording(
    target: Target,
    input: Input,
    timeline: &mut Timeline,
    dictation: Option<Dictation>,
    keep_take: bool,
) -> Phase {
    let start = target.start_time();
    // A presentation that starts on a stop waits there first.
    let waiting_at = (target.presents()
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
        dictation,
        keep_take,
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
    let Some(files) = target.files().cloned() else {
        return;
    };
    let sidecar = match &target {
        Target::Voiceover { .. } => TakeSidecar {
            markers: taps.into_iter().collect::<BTreeMap<_, _>>(),
            ..TakeSidecar::default()
        },
        Target::Live { .. } | Target::Dictation { .. } => TakeSidecar {
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
    paths: Option<Res<ProjectPaths>>,
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
    let slides = script_slides(&stash);
    // gaanim.toml changes re-run the scene, so a replay is when to read it.
    if panel
        .speech
        .as_ref()
        .is_none_or(|(revision, _)| *revision != stash.revision)
    {
        panel.speech = Some((stash.revision, ProjectSpeech::read(paths.as_deref())));
    }
    let project = panel
        .speech
        .as_ref()
        .map(|(_, speech)| speech.clone())
        .unwrap_or_default();
    let mut close = false;
    let mut request = None;
    let mut settings_changed = false;
    let mut vault_action = None;
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
                            egui::RichText::new("Guion y narración")
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
                            if !slides.is_empty() {
                                slides_hint(ui);
                            } else if manifest.is_empty() {
                                empty_state(ui);
                            }
                            script_card(ui, &manifest, &slides, script.as_deref(), &mut request);
                            if let Some(live) = &manifest.live {
                                ui.label(caption("TOMA EN VIVO"));
                                let busy = panel.busy(&live.key);
                                live_card(ui, live, busy, &mut panel.dictate_live, &mut request);
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
                            let NarrationPanel {
                                settings,
                                scribe_token,
                                token_stored,
                                ..
                            } = &mut *panel;
                            let settings = settings.get_or_insert_with(NarrationSettings::load);
                            let stored = *token_stored
                                .get_or_insert_with(|| scribe::vault::load().is_some());
                            let (changed, action) =
                                scribe_settings(ui, settings, scribe_token, stored, &project);
                            settings_changed |= changed;
                            vault_action = action;
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
    match vault_action {
        Some(VaultAction::Save) => {
            let saved = ScribeToken::new(&panel.scribe_token)
                .ok_or_else(|| "pega el token antes de guardarlo".to_owned())
                .and_then(|token| scribe::vault::save(&token));
            match saved {
                Ok(()) => {
                    panel.scribe_token.clear();
                    panel.token_stored = Some(true);
                    panel.notify(
                        NoticeKind::Success,
                        "Token de Scribe guardado en el Administrador de credenciales de Windows",
                    );
                }
                Err(error) => panel.notify(NoticeKind::Error, error),
            }
        }
        Some(VaultAction::Forget) => match scribe::vault::forget() {
            Ok(()) => {
                panel.token_stored = Some(false);
                panel.notify(NoticeKind::Info, "Token de Scribe quitado del almacén");
            }
            Err(error) => panel.notify(NoticeKind::Error, error),
        },
        Some(VaultAction::Pin(language)) => {
            let pinned = language.clone();
            write_project_language(&mut panel, &project, Some(language), move || {
                format!("Idioma fijado en gaanim.toml: language = \"{pinned}\"")
            });
        }
        Some(VaultAction::Unpin) => {
            write_project_language(&mut panel, &project, None, || {
                "gaanim.toml ya no fija el idioma: se usa el tuyo".to_owned()
            });
        }
        None => {}
    }
}

/// Write the project's `language`, or remove it, and say so.
fn write_project_language(
    panel: &mut NarrationPanel,
    project: &ProjectSpeech,
    language: Option<String>,
    done: impl FnOnce() -> String,
) {
    let Some(manifest) = &project.manifest else {
        panel.notify(
            NoticeKind::Error,
            "esta escena no pertenece a un proyecto con gaanim.toml",
        );
        return;
    };
    match gaanim_project::set_project_language(manifest, language.as_deref()) {
        Ok(()) => {
            // Read it again on the next frame rather than wait for the replay.
            panel.speech = None;
            panel.notify(NoticeKind::Success, done());
        }
        Err(error) => panel.notify(NoticeKind::Error, error),
    }
}

/// What the Scribe settings ask for: the credential store, or the
/// project's language.
#[derive(Debug, Clone, PartialEq, Eq)]
enum VaultAction {
    Save,
    Forget,
    Pin(String),
    Unpin,
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

fn slides_hint(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(
            "El guion de la charla: una sección «## nombre» por diapositiva con lo que \
             dirás, y una línea [stop] donde avanzas. Presenter View lo muestra en sus \
             notas y, con T, como teleprompter. Gaanim añade las diapositivas y los \
             stops que falten sin tocar tu texto.",
        )
        .size(12.5)
        .color(palette::TEXT_MUTED),
    );
}

/// Where the slides' notes and the voiceover texts live, with a button to
/// write them.
fn script_card(
    ui: &mut egui::Ui,
    manifest: &NarrationManifest,
    slides: &[ScriptSlide],
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
                    "{} · {} secciones",
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
        let sections = manifest
            .script
            .as_ref()
            .map_or(&[][..], |script| &script.sections[..]);
        let missing = script_gaps(sections, manifest, slides);
        let label = match (&manifest.script, missing) {
            (None, _) => "Crear guion".to_string(),
            (Some(_), 0) => "Abrir guion".to_string(),
            (Some(_), missing) => format!("Abrir y completar ({missing})"),
        };
        ui.horizontal_wrapped(|ui| {
            if secondary_button(ui, &label, path.is_some())
                .on_hover_text(
                    "Escribe bajo \"## nombre\" lo que dices en cada diapositiva o bloque de \
                     voz, con [stop] donde avanzas; al guardar, la escena se actualiza",
                )
                .clicked()
            {
                *request = Some(Request::OpenScript);
            }
            if !slides.is_empty()
                && secondary_button(ui, "Dictar con Scribe", path.is_some())
                    .on_hover_text(
                        "Presenta desde la diapositiva actual hablando: Scribe de Zoom \
                         transcribe tu voz y cada frase va a su diapositiva y su tramo del \
                         guion, sin pisar lo que ya escribiste",
                    )
                    .clicked()
            {
                *request = Some(Request::Dictate);
            }
        });
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
    dictate: &mut bool,
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
                 Enter termina. El teleprompter muestra el guion de cada diapositiva \
                 y marca dónde avanzar.",
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
                *request = Some(Request::RecordLive { dictate: *dictate });
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
        ui.checkbox(dictate, "Dictar también el guion con Scribe")
            .on_hover_text(
                "Mientras grabas, Scribe de Zoom transcribe tu voz y cada frase va a su \
                 diapositiva en el guion (Ajustes → Scribe de Zoom)",
            );
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

/// Scribe's preferences, saved with the others, and the token for this
/// session, which is not.
fn scribe_settings(
    ui: &mut egui::Ui,
    settings: &mut NarrationSettings,
    typed_token: &mut String,
    stored: bool,
    project: &ProjectSpeech,
) -> (bool, Option<VaultAction>) {
    let before = settings.clone();
    let mut action = None;
    egui::CollapsingHeader::new(
        egui::RichText::new("Scribe de Zoom (dictado)")
            .size(12.5)
            .color(palette::TEXT_MUTED),
    )
    .id_salt("narration_scribe_settings")
    .show(ui, |ui| {
        let faint = |text: String| {
            egui::RichText::new(text)
                .size(11.5)
                .color(palette::TEXT_FAINT)
        };
        ui.add(
            egui::Label::new(faint(
                "Dicta el guion presentando: Scribe Live de Zoom transcribe tu voz y cada \
                 frase va a la diapositiva y el tramo que presentabas."
                    .to_owned(),
            ))
            .wrap(),
        );
        // The stored token is looked up once, not every frame.
        let resolved = ScribeToken::resolve(
            typed_token,
            || stored.then(|| ScribeToken::new("stored")).flatten(),
            |name| std::env::var(name).ok(),
        );
        let (color, status) = match resolved.map(|(_, source)| source) {
            Some(TokenSource::Typed) => (palette::LOOP, "Token pegado para esta sesión".to_owned()),
            Some(TokenSource::Vault) => (
                palette::LOOP,
                "Token guardado en el Administrador de credenciales de Windows".to_owned(),
            ),
            Some(TokenSource::Environment) => (
                palette::LOOP,
                format!("Token de la variable {}", scribe::TOKEN_VARIABLE),
            ),
            None => (palette::TEXT_FAINT, "Sin token: pégalo aquí".to_owned()),
        };
        ui.label(egui::RichText::new(status).size(11.5).color(color));
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(typed_token)
                    .password(true)
                    .hint_text("Pegar token")
                    .desired_width(180.0),
            );
            if !typed_token.is_empty() {
                if scribe::vault::AVAILABLE
                    && small_button(ui, "Guardar en Windows", true)
                        .on_hover_text(
                            "Lo cifra el Administrador de credenciales de Windows para tu \
                             usuario; nunca va a la carpeta del proyecto",
                        )
                        .clicked()
                {
                    action = Some(VaultAction::Save);
                }
                if small_button(ui, "Olvidar", true)
                    .on_hover_text("Lo quita de esta sesión")
                    .clicked()
                {
                    typed_token.clear();
                }
            } else if stored
                && small_button(ui, "Quitar el guardado", true)
                    .on_hover_text("Lo borra del Administrador de credenciales de Windows")
                    .clicked()
            {
                action = Some(VaultAction::Forget);
            }
        });
        // The project's language wins; changing it then rewrites gaanim.toml.
        let current = dictation_language(project, settings);
        let pinned = project.locale().is_some();
        let mut chosen = current;
        ui.horizontal(|ui| {
            section_label(ui, "Idioma");
            egui::ComboBox::from_id_salt("narration_scribe_language")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for language in scribe::LANGUAGES {
                        ui.selectable_value(&mut chosen, language, language);
                    }
                });
            if pinned {
                if small_button(ui, "Desfijar", true)
                    .on_hover_text("Quita language de gaanim.toml y usa tu idioma")
                    .clicked()
                {
                    action = Some(VaultAction::Unpin);
                }
            } else if project.manifest.is_some()
                && small_button(ui, "Fijar en gaanim.toml", true)
                    .on_hover_text(
                        "Escribe language en el gaanim.toml del proyecto: la charla se dicta \
                         en este idioma en cualquier equipo",
                    )
                    .clicked()
            {
                action = Some(VaultAction::Pin(current.to_owned()));
            }
        });
        if chosen != current {
            if pinned {
                action = Some(VaultAction::Pin(chosen.to_owned()));
            } else {
                settings.scribe_language = chosen.to_owned();
            }
        }
        match (&project.language, project.locale()) {
            (Some(tag), Some(locale)) if !tag.eq_ignore_ascii_case(locale) => {
                ui.label(faint(format!(
                    "Fijado en gaanim.toml como «{tag}»; Scribe lo dicta como {locale}"
                )));
            }
            (Some(_), Some(_)) => {
                ui.label(faint("Fijado en gaanim.toml (language)".to_owned()));
            }
            (Some(tag), None) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "gaanim.toml pide «{tag}», que Scribe no transcribe: se usa {current}"
                        ))
                        .size(11.5)
                        .color(palette::STOP),
                    )
                    .wrap(),
                );
            }
            (None, _) => {}
        }
        if !project.secret_keys.is_empty() {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(format!(
                        "gaanim.toml tiene {} con pinta de credencial. Gaanim no la lee: \
                         quítala del proyecto y, si llegó a un repositorio, genera otra clave.",
                        project
                            .secret_keys
                            .iter()
                            .map(|key| format!("`{key}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .size(11.5)
                    .color(palette::DANGER),
                )
                .wrap(),
            );
        }
        ui.horizontal(|ui| {
            section_label(ui, "Minutos máximos por sesión");
            ui.add(
                egui::DragValue::new(&mut settings.scribe_max_minutes)
                    .range(1..=scribe::MAX_SESSION_MINUTES)
                    .suffix(" min"),
            );
        });
        ui.checkbox(
            &mut settings.scribe_vocabulary,
            "Enviar los nombres de diapositivas y stops como vocabulario",
        );
        ui.add(
            egui::Label::new(faint(format!(
                "Mientras dictas, tu voz se envía a Zoom{}. El token nunca va a la carpeta \
                 del proyecto ni aparece en mensajes: pegado vive solo en esta sesión, y \
                 guardado lo cifra Windows. La sesión se corta sola al llegar al límite de \
                 minutos o tras dos minutos sin voz, y nunca se reconecta sola.",
                if settings.scribe_vocabulary {
                    ", con los nombres de las diapositivas"
                } else {
                    ""
                }
            )))
            .wrap(),
        );
    });
    (*settings != before, action)
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
            && let Some(path) =
                crate::platform::pick_file(title, None, &[(title, extensions), ("Todos", &["*"])])
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
            plan,
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
                let starting = match (target, plan.scribe.is_some()) {
                    (Target::Dictation { .. }, _) => "Dictando el guion en un momento".to_owned(),
                    (_, true) => format!("Grabando «{}» y dictando en un momento", target.key()),
                    (_, false) => format!("Grabando «{}» en un momento", target.key()),
                };
                ui.label(
                    egui::RichText::new(starting)
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
        Phase::Finishing { dictation, .. } => {
            centered_card(ctx, |ui| {
                ui.label(
                    egui::RichText::new("Terminando el dictado…")
                        .size(16.0)
                        .color(palette::TEXT),
                );
                ui.label(
                    egui::RichText::new(format!(
                        "{} frase(s) recibida(s) · Esc deja de esperar",
                        dictation.turns.len()
                    ))
                    .size(12.0)
                    .color(palette::TEXT_MUTED),
                );
            });
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
                        let title = match (recording_now, target) {
                            (true, Target::Dictation { .. }) => "DICTANDO",
                            (true, _) => "GRABANDO",
                            (false, _) => "PREPARADO",
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
                        Target::Live { .. } | Target::Dictation { .. } => {
                            let position = ScriptPosition::of(timeline);
                            if let Some(position) = position
                                && !*compact
                            {
                                ui.label(
                                    egui::RichText::new(&timeline.segments[position.slide].name)
                                        .size(13.0)
                                        .color(palette::TEXT_MUTED),
                                );
                                let lines = script_lines(
                                    &timeline.segments,
                                    position.slide..position.slide + 1,
                                    Some(position),
                                    false,
                                );
                                let anchor_id = egui::Id::new("live-teleprompter-anchor");
                                let anchor = (position.slide, position.part);
                                let follow = ui
                                    .data(|data| data.get_temp::<(usize, usize)>(anchor_id))
                                    != Some(anchor);
                                ui.data_mut(|data| data.insert_temp(anchor_id, anchor));
                                egui::ScrollArea::vertical()
                                    .id_salt("live-teleprompter")
                                    .max_height((screen.height() * 0.35).max(160.0))
                                    .show(ui, |ui| {
                                        show_script(
                                            ui,
                                            &lines,
                                            24.0,
                                            follow,
                                            &PrompterWords::SPANISH,
                                        );
                                    });
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
                    if let Some(dictation) =
                        recording.and_then(|recording| recording.dictation.as_ref())
                    {
                        dictation_status(ui, dictation);
                    }
                    let hints = if target.presents() {
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

/// How the dictation goes: Scribe's state and the last turn it heard.
fn dictation_status(ui: &mut egui::Ui, dictation: &Dictation) {
    let (color, state) = match (&dictation.note, dictation.closed, dictation.ready) {
        (Some((note, true)), ..) => (palette::DANGER, note.clone()),
        (Some((note, false)), ..) => (palette::STOP, note.clone()),
        (None, true, _) => (palette::TEXT_MUTED, "Scribe terminó".to_owned()),
        (None, false, false) => (palette::TEXT_MUTED, "Scribe: conectando…".to_owned()),
        (None, false, true) if dictation.speaking => {
            (palette::LOOP, "Scribe: te escucha…".to_owned())
        }
        (None, false, true) => (palette::LOOP, "Scribe: listo".to_owned()),
    };
    ui.label(
        egui::RichText::new(format!("{state} · {} frase(s)", dictation.turns.len()))
            .size(13.0)
            .color(color),
    );
    if let Some((_, text)) = dictation.turns.last() {
        ui.add(
            egui::Label::new(
                egui::RichText::new(format!("«{}»", truncate_chars(text, 140)))
                    .size(13.0)
                    .italics()
                    .color(palette::TEXT_FAINT),
            )
            .wrap(),
        );
    }
}

/// `text` cut to `limit` characters, with an ellipsis when cut.
fn truncate_chars(text: &str, limit: usize) -> String {
    match text.char_indices().nth(limit) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
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
            plan: CapturePlan::TAKE,
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
        assert_eq!(complete_script(&path, &manifest, &[]).unwrap().sections, 2);
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
        assert_eq!(complete_script(&path, &manifest, &[]).unwrap().sections, 1);
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
        assert!(complete_script(&path, &manifest, &[]).unwrap().is_empty());
    }

    fn slide(name: &str, notes: Option<&str>, stops: &[Option<&str>]) -> ScriptSlide {
        ScriptSlide {
            name: name.into(),
            notes: notes.map(str::to_owned),
            stops: stops.iter().map(|stop| stop.map(str::to_owned)).collect(),
        }
    }

    #[test]
    fn the_script_gets_a_section_per_slide_and_a_mark_per_stop() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("narration").join(DEFAULT_SCRIPT);
        let manifest = NarrationManifest::default();
        let mut slides = vec![
            slide("Portada", Some("Hola a todos."), &[Some("titulo")]),
            slide("Datos", None, &[Some("grafico"), None]),
        ];
        let added = complete_script(&path, &manifest, &slides).unwrap();
        assert_eq!(
            added,
            ScriptAdditions {
                sections: 2,
                marks: 3,
                from_notes: 1
            }
        );
        let notes = |path: &Path| {
            parse_script(&std::fs::read_to_string(path).unwrap())
                .unwrap()
                .into_iter()
                .map(|section| (section.key, section.notes))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            notes(&path),
            [
                (
                    "Portada".to_owned(),
                    "Hola a todos.\n\n[stop titulo]".to_owned()
                ),
                ("Datos".to_owned(), "[stop grafico]\n\n[stop]".to_owned()),
            ]
        );
        let sections = parse_script(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(script_gaps(&sections, &manifest, &slides), 0);

        // The author writes the script; the code gains a slide and a stop.
        let written = std::fs::read_to_string(&path)
            .unwrap()
            .replace("## Datos\n", "## datos\n\nMiren el gráfico.\n")
            .replace('\n', "\r\n");
        std::fs::write(&path, written).unwrap();
        slides[0].stops.push(Some("logo".into()));
        slides.insert(1, slide("Método", None, &[]));
        let sections = parse_script(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(script_gaps(&sections, &manifest, &slides), 2);
        let added = complete_script(&path, &manifest, &slides).unwrap();
        assert_eq!(
            added,
            ScriptAdditions {
                sections: 1,
                marks: 1,
                from_notes: 0
            }
        );
        let source = std::fs::read_to_string(&path).unwrap();
        assert!(!source.replace("\r\n", "").contains('\n'), "keeps CRLF");
        assert_eq!(
            notes(&path),
            [
                (
                    "Portada".to_owned(),
                    "Hola a todos.\n\n[stop titulo]\n\n[stop logo]".to_owned()
                ),
                ("Método".to_owned(), String::new()),
                (
                    "datos".to_owned(),
                    "Miren el gráfico.\n\n[stop grafico]\n\n[stop]".to_owned()
                ),
            ]
        );
        assert!(
            complete_script(&path, &manifest, &slides)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn dictation_fills_empty_parts_and_comments_beside_written_ones() {
        let slides = vec![
            slide("Portada", None, &[Some("titulo")]),
            slide("Datos", None, &[Some("grafico"), None]),
        ];
        let existing = "# Guion\n\n## Datos\n\nYa escrito.\n\n[stop grafico]\n\n[stop]\n";
        let dictated = vec![
            ("Portada".to_owned(), 0, "Hola a todos.".to_owned()),
            ("datos".to_owned(), 0, "Miren --> esto.".to_owned()),
            ("Datos".to_owned(), 1, "Crece.".to_owned()),
            ("Datos".to_owned(), 2, "Gracias.".to_owned()),
            ("Otra".to_owned(), 0, "Nada.".to_owned()),
        ];
        let (source, report) = merge_dictation(Some(existing), &slides, &dictated).unwrap();
        assert_eq!(
            report,
            DictationReport {
                parts: 4,
                commented: 1
            }
        );
        let notes: Vec<(String, String)> = parse_script(&source)
            .unwrap()
            .into_iter()
            .map(|section| (section.key, section.notes))
            .collect();
        assert_eq!(
            notes,
            [
                (
                    "Portada".to_owned(),
                    "Hola a todos.\n\n[stop titulo]".to_owned()
                ),
                (
                    "Datos".to_owned(),
                    "Ya escrito.\n\n[stop grafico]\n\nCrece.\n\n[stop]\n\nGracias.".to_owned()
                ),
            ]
        );
        // The written text stays; the dictation sits beside it, closed once.
        assert!(
            source.contains("<!-- dictado: Miren –> esto. -->"),
            "{source}"
        );
        assert!(source.contains("[stop titulo]\n\n## Datos"), "{source}");
    }

    #[test]
    fn the_project_language_wins_over_the_speakers() {
        let mut settings = NarrationSettings {
            scribe_language: "en-US".into(),
            ..NarrationSettings::default()
        };
        let mut project = ProjectSpeech::default();
        assert_eq!(dictation_language(&project, &settings), "en-US");
        project.language = Some("es-PE".into());
        assert_eq!(dictation_language(&project, &settings), "es-ES");
        // A language Scribe lacks leaves the speaker's.
        project.language = Some("ko".into());
        assert_eq!(dictation_language(&project, &settings), "en-US");
        settings.scribe_language = "xx".into();
        project.language = None;
        assert_eq!(dictation_language(&project, &settings), "es-ES");
    }

    #[test]
    fn the_scribe_token_is_never_part_of_the_saved_settings() {
        let mut panel = NarrationPanel {
            scribe_token: "secret-token".to_owned(),
            ..NarrationPanel::default()
        };
        let saved = serde_json::to_string(panel.settings()).unwrap();
        assert!(
            !saved.contains("secret-token") && !saved.contains("token"),
            "{saved}"
        );
        let vocabulary = vocabulary(&[
            slide("Portada", None, &[Some("titulo"), None]),
            slide("portada", None, &[Some(" Titulo ")]),
        ]);
        assert_eq!(vocabulary, ["Portada", "titulo"]);
    }
}
