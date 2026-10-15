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
//!
//! Recording the talk times it: each slide the speaker goes through whole
//! keeps how long it took, against its target and the talk's planned
//! length, and one slide can be recorded again on its own. The caption at
//! the bottom of the preview is the prompter and the recorder's display at
//! once (see `presenter::prompter`).

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

use crate::presenter::pace::{self, Rehearsals, SlideTiming};
use crate::presenter::prompter::{
    self, CaptionText, Cue, IMPLICIT_SEGMENT, PrompterPace, PrompterSettings, ScriptPosition,
};
use scribe::{
    ScribeConfig, ScribeEvent, ScribeSession, ScribeToken, StopReason, TokenSource, Turn,
};

use crate::export::{ProjectPaths, StashedReplay};
use crate::ui_kit::{
    ButtonSize, ButtonTone, Icon, caption, card_frame, field_frame, icon_button, paint_icon,
    palette, primary_button_sized, secondary_button_sized, section_label, small_button_sized,
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

/// Script & narration is read while speaking, a step away from the
/// screen: it is wider and larger than the editor's other panels.
const PANEL_WIDTH: f32 = 470.0;
/// The size of its small buttons' text.
const SMALL_TEXT: f32 = 14.5;

/// The styles of Script & narration's widgets, a size up from the editor's.
fn enlarge(style: &mut egui::Style) {
    for font in style.text_styles.values_mut() {
        font.size = (font.size * 1.15).round();
    }
    style.spacing.item_spacing.y = 10.0;
    style.spacing.interact_size.y = 30.0;
    style.spacing.icon_width = 18.0;
    style.spacing.icon_width_inner = 10.0;
    style.spacing.icon_spacing = 8.0;
}

fn primary(ui: &mut egui::Ui, label: &str, icon: Option<Icon>, enabled: bool) -> egui::Response {
    primary_button_sized(ui, label, icon, enabled, ButtonSize::LARGE)
}

fn secondary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    secondary_button_sized(ui, label, enabled, ButtonSize::LARGE)
}

fn small(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    small_button_sized(ui, label, enabled, SMALL_TEXT)
}

/// A section's heading.
fn heading(text: &str) -> egui::RichText {
    caption(text).size(12.5)
}

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
            Self::Hot => "Too loud! Lower the gain or move away from the microphone".to_string(),
            Self::Clipped(1) => {
                "Clipped once: that distortion cannot be fixed; consider redoing the take"
                    .to_string()
            }
            Self::Clipped(count) => format!(
                "Clipped {count} times: that distortion cannot be fixed; consider redoing the take"
            ),
            Self::Quiet => {
                "Voice too low: move closer to the microphone or raise the gain".to_string()
            }
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
        Err(detail.unwrap_or("no details").trim().to_string())
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
    /// A Scribe token typed for this session. It is never saved or shown.
    scribe_token: String,
    /// Whether the credential store holds a token, once looked up.
    token_stored: Option<bool>,
    /// What the project's gaanim.toml says, read again on every replay.
    speech: Option<(u64, ProjectSpeech)>,
    /// The same, for Settings: read when it opens.
    settings_speech: Option<ProjectSpeech>,
    /// Write what the speaker says into the script with Scribe while
    /// recording; unset, whenever a token is at hand.
    dictate: Option<bool>,
    /// Slides whose stops the timing card lists, by name.
    expanded: std::collections::BTreeSet<String>,
    /// The kept audio files of the talk, and when they were listed.
    clip_files: Option<(Instant, Vec<String>)>,
}

/// The sections of Settings the narration preferences fill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NarrationSettingsSection {
    Recording,
    Transcription,
    Dictation,
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
    /// Present the talk to time it, dictating its script with Scribe when
    /// `dictate`: from the slide named `from` (the first when `None`), or
    /// from its part `part`, and only that slide or part when `only`.
    RecordTalk {
        from: Option<String>,
        part: Option<usize>,
        only: bool,
        dictate: bool,
    },
    /// Move the playhead there, paused.
    Seek(f64),
    Listen(f64),
    Transcribe(String),
    Level(String),
    OpenScript,
}

impl NarrationPanel {
    fn settings(&mut self) -> &mut NarrationSettings {
        self.settings.get_or_insert_with(NarrationSettings::load)
    }

    /// Whether a Scribe token is at hand: typed, saved or in the
    /// environment.
    fn token_available(&mut self) -> bool {
        !self.scribe_token.trim().is_empty()
            || *self
                .token_stored
                .get_or_insert_with(|| scribe::vault::load().is_some())
            || std::env::var(scribe::TOKEN_VARIABLE).is_ok_and(|token| !token.trim().is_empty())
    }

    /// The names of the files in `dir`, listed again every second.
    fn clip_files(&mut self, dir: Option<&Path>) -> Vec<String> {
        let stale = self
            .clip_files
            .as_ref()
            .is_none_or(|(listed, _)| listed.elapsed() > Duration::from_secs(1));
        if stale {
            let files = dir
                .and_then(|dir| std::fs::read_dir(dir).ok())
                .into_iter()
                .flatten()
                .flatten()
                .filter_map(|entry| entry.file_name().into_string().ok())
                .collect();
            self.clip_files = Some((Instant::now(), files));
        }
        self.clip_files
            .as_ref()
            .map(|(_, files)| files.clone())
            .unwrap_or_default()
    }

    /// Whether recordings dictate the script.
    fn dictating(&mut self) -> bool {
        match self.dictate {
            Some(dictate) => dictate,
            None => self.token_available(),
        }
    }

    /// Read the project's gaanim.toml again the next time Settings shows it.
    pub(crate) fn forget_settings_project(&mut self) {
        self.settings_speech = None;
    }

    /// A section of Settings: the preferences, saved as soon as they change.
    pub(crate) fn settings_ui(
        &mut self,
        ui: &mut egui::Ui,
        section: NarrationSettingsSection,
        paths: Option<&ProjectPaths>,
    ) {
        let changed = match section {
            NarrationSettingsSection::Recording => recording_form(ui, self.settings()),
            NarrationSettingsSection::Transcription => whisper_form(ui, self.settings()),
            NarrationSettingsSection::Dictation => {
                let project = self
                    .settings_speech
                    .get_or_insert_with(|| ProjectSpeech::read(paths))
                    .clone();
                let stored = *self
                    .token_stored
                    .get_or_insert_with(|| scribe::vault::load().is_some());
                let settings = self.settings.get_or_insert_with(NarrationSettings::load);
                let (changed, action) =
                    scribe_settings(ui, settings, &mut self.scribe_token, stored, &project);
                if let Some(action) = action {
                    apply_vault_action(self, &project, action);
                    self.settings_speech = None;
                }
                changed
            }
        };
        if changed && let Err(error) = self.settings().save() {
            self.notify(
                NoticeKind::Error,
                format!("the settings were not saved: {error}"),
            );
        }
        if let Some((message, kind)) = &self.notice {
            ui.add_space(6.0);
            ui.add(
                egui::Label::new(
                    egui::RichText::new(message)
                        .size(14.0)
                        .color(notice_color(*kind)),
                )
                .wrap(),
            );
        }
    }

    /// The speaker's pace measured from their dictations.
    pub(crate) fn measured_pace(&mut self) -> Option<f64> {
        self.settings().speaking_wpm
    }

    /// Forget the measured pace; the next dictation measures it again.
    pub(crate) fn forget_measured_pace(&mut self) -> Result<(), String> {
        self.settings().speaking_wpm = None;
        self.settings().save()
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
                    format!("Transcribing \"{key}\" with Whisper…"),
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
            self.notify(NoticeKind::Error, format!("\"{key}\" has no take yet"));
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
                        format!("could not keep the original of \"{key}\": {error}"),
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
                    format!("Leveling \"{key}\" to {target:.0} LUFS…"),
                );
            }
            Err(error) => self.notify(NoticeKind::Error, format!("could not level: {error}")),
        }
    }

    fn poll_jobs(&mut self) {
        let mut finished = Vec::new();
        self.transcriptions.retain(|job| match job.poll() {
            Some(result) => {
                finished.push(match result {
                    Ok(words) => (
                        NoticeKind::Success,
                        format!("\"{}\" transcribed: {words} timed words", job.key),
                    ),
                    Err(error) => (
                        NoticeKind::Error,
                        format!("Whisper (\"{}\"): {error}", job.key),
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
                            "\"{}\" leveled to {target:.0} LUFS (it was at {:.1})",
                            job.key, measured.integrated
                        ),
                    ),
                    Err(error) => (
                        NoticeKind::Error,
                        format!("\"{}\" was saved without leveling: {error}", job.key),
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
    /// Present the talk to time it and dictate its script; no take is
    /// recorded. With `only`, one slide or part, and the recording ends
    /// once the speaker leaves it.
    Talk {
        start_time: f64,
        only: Option<Scope>,
    },
}

/// What recording the talk records again: one slide, or one part of it.
#[derive(Debug, Clone, PartialEq)]
struct Scope {
    slide: String,
    /// The part, by index: the text said before its stop.
    part: Option<usize>,
}

impl Scope {
    fn covers(&self, slide: &str, part: usize) -> bool {
        self.slide == slide && self.part.is_none_or(|own| own == part)
    }
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
            Self::Talk { .. } => "talk",
        }
    }

    fn start_time(&self) -> f64 {
        match self {
            Self::Voiceover { start_time, .. }
            | Self::Live { start_time, .. }
            | Self::Talk { start_time, .. } => *start_time,
        }
    }

    fn files(&self) -> Option<&TakeFiles> {
        match self {
            Self::Voiceover { files, .. } | Self::Live { files, .. } => Some(files),
            Self::Talk { .. } => None,
        }
    }

    fn is_live(&self) -> bool {
        matches!(self, Self::Live { .. })
    }

    /// Whether the speaker presents the scene, pausing at every stop.
    fn presents(&self) -> bool {
        matches!(self, Self::Live { .. } | Self::Talk { .. })
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
    /// Transcribed phrases, on the recording clock.
    turns: Vec<Turn>,
    ready: bool,
    speaking: bool,
    /// A warning from Scribe, or why the session stopped or failed.
    note: Option<(String, bool)>,
    closed: bool,
    /// Where the timeline was when Scribe stopped before the speaker
    /// finished: nothing after it was transcribed.
    stopped_at: Option<f64>,
}

impl Dictation {
    fn new(session: ScribeSession) -> Self {
        Self {
            session,
            turns: Vec::new(),
            ready: false,
            speaking: false,
            note: None,
            closed: false,
            stopped_at: None,
        }
    }

    /// Take in what the session reported.
    fn absorb(&mut self) {
        for event in self.session.poll() {
            match event {
                ScribeEvent::Ready => self.ready = true,
                ScribeEvent::Speech(speaking) => self.speaking = speaking,
                ScribeEvent::Turn { start, end, text } => {
                    self.speaking = false;
                    self.turns.push(Turn { start, end, text });
                }
                ScribeEvent::Warning(message) => self.note = Some((message, false)),
                ScribeEvent::Stopping(reason) => {
                    self.note = Some((
                        match reason {
                            StopReason::Limit => {
                                "Scribe stopped: the minute limit was reached".to_owned()
                            }
                            StopReason::Silence => {
                                "Scribe stopped after two minutes without speech".to_owned()
                            }
                            StopReason::NoAudio => {
                                "Scribe stopped: the microphone stopped sending audio".to_owned()
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

    /// What has been transcribed of part `part` of the slide named `slide`.
    fn transcript(&self, track: &pace::ScriptTrack, slide: &str, part: usize) -> String {
        scribe::dictated_parts(track, &self.turns)
            .into_iter()
            .find(|(name, index, _)| name == slide && *index == part)
            .map(|(_, _, text)| text)
            .unwrap_or_default()
    }

    /// How Scribe is doing, in a few words, and its color.
    fn state(&self) -> (String, egui::Color32) {
        match (&self.note, self.closed, self.ready) {
            (Some((note, true)), ..) => (note.clone(), palette::DANGER),
            (Some((note, false)), ..) => (note.clone(), palette::STOP),
            (None, true, _) => ("Scribe finished".to_owned(), palette::TEXT_MUTED),
            (None, false, false) => ("Scribe: connecting…".to_owned(), palette::TEXT_MUTED),
            (None, false, true) => (
                format!("Scribe · {} phrase(s)", self.turns.len()),
                palette::LOOP,
            ),
        }
    }
}

/// The slide part the speaker is at on `timeline`: the slide by name.
fn script_place(timeline: &Timeline) -> Option<(String, usize)> {
    ScriptPosition::of(timeline).map(|position| {
        (
            timeline.segments[position.slide].name.clone(),
            position.part,
        )
    })
}

/// Whether a recording that followed `track` went through `scope` and has
/// left it.
fn left_scope(track: &pace::ScriptTrack, scope: &Scope) -> bool {
    let inside = |entry: &(f64, Option<(String, usize)>)| {
        entry
            .1
            .as_ref()
            .is_some_and(|(slide, part)| scope.covers(slide, *part))
    };
    track.iter().any(inside) && track.last().is_some_and(|last| !inside(last))
}

/// The name of part `part` of the slide named `slide`: its stop's, else
/// its number, or what follows the last stop.
fn part_label(segment: &gaanim_timeline::timeline::SegmentMetadata, part: usize) -> String {
    match segment.stops.get(part) {
        Some(stop) => stop
            .name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map_or_else(|| format!("Step {}", part + 1), str::to_owned),
        None => "until the next slide".to_owned(),
    }
}

/// The parts of `segment` its stops cut it into: one before each stop, and
/// one after the last when the slide goes on past it.
fn slide_parts(segment: &gaanim_timeline::timeline::SegmentMetadata) -> usize {
    let after = segment
        .stops
        .last()
        .is_some_and(|stop| segment.end_time - stop.time > 0.05);
    segment.stops.len() + usize::from(after)
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
    /// Where the speaker was in the script, on the recording clock.
    track: Vec<(f64, Option<(String, usize)>)>,
    /// The recording began where its first slide begins.
    entered_first: bool,
}

impl Recording {
    fn next_marker(&self) -> Option<&str> {
        match &self.target {
            Target::Voiceover { markers, .. } => markers.get(self.taps.len()).map(String::as_str),
            Target::Live { .. } | Target::Talk { .. } => None,
        }
    }

    /// Note where the speaker is in the script, `elapsed` seconds into the
    /// recording.
    fn follow(&mut self, timeline: &Timeline, elapsed: f64) {
        // Until a seek lands, the playhead is still where it was.
        if timeline.seek_request.is_some() {
            return;
        }
        let place = script_place(timeline);
        if self.track.last().map(|(_, last)| last) != Some(&place) {
            self.track.push((elapsed, place));
        }
    }

    /// The slide being said and the seconds since the speaker entered it,
    /// at `elapsed` seconds into the recording.
    fn on_slide(&self, elapsed: f64) -> Option<(&str, f64)> {
        let (_, Some((slide, _))) = self.track.last()? else {
            return None;
        };
        let entered = self
            .track
            .iter()
            .rev()
            .take_while(|(_, place)| place.as_ref().is_some_and(|(name, _)| name == slide))
            .last()
            .map_or(elapsed, |(time, _)| *time);
        Some((slide.as_str(), elapsed - entered))
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
        track: Vec<(f64, Option<(String, usize)>)>,
        /// The slide or part recorded again: the dictation goes only there.
        scope: Option<Scope>,
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

/// Complete the script at `path` (see [`complete_script`]) and open it in
/// the code editor, as every other jump to the code, at [`script_line`].
/// Returns what completing added and whether the editor opened.
fn complete_and_open(
    path: &Path,
    manifest: &NarrationManifest,
    slides: &[ScriptSlide],
) -> Result<(ScriptAdditions, std::io::Result<()>), String> {
    let before = std::fs::read_to_string(path).ok();
    let added = complete_script(path, manifest, slides)?;
    let line = std::fs::read_to_string(path)
        .map_or(1, |completed| script_line(before.as_deref(), &completed));
    let location =
        gaanim_core::console::ScriptLocation::new(path, u32::try_from(line).unwrap_or(1));
    Ok((added, crate::source_link::open(&location)))
}

/// The 1-based line of `completed` to open the script at: the first line
/// completing it added to `before` (the script before, `None` when it did
/// not exist), else the first section with nothing to read, else its first
/// section.
fn script_line(before: Option<&str>, completed: &str) -> usize {
    let sections = parse_script(completed).unwrap_or_default();
    let first_section = sections.first().map_or(1, |section| section.line);
    let Some(before) = before else {
        return first_section;
    };
    // Completing only inserts lines, so the first line that differs starts
    // the first insertion, which opens with a blank line.
    let after: Vec<&str> = completed.lines().collect();
    if let Some(changed) = after
        .iter()
        .zip(before.lines())
        .position(|(after, before)| after != &before)
        .or_else(|| (after.len() > before.lines().count()).then(|| before.lines().count()))
    {
        let written = after[changed..]
            .iter()
            .position(|line| !line.trim().is_empty())
            .map_or(changed, |offset| changed + offset);
        return written + 1;
    }
    sections
        .iter()
        .find(|section| section.text.trim().is_empty())
        .map_or(first_section, |section| section.line)
}

/// What dictating did to the script.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DictationReport {
    /// Slide parts that received dictated text.
    parts: usize,
    /// Of those, parts whose text it replaced, keeping that text in a
    /// comment.
    replaced: usize,
    /// Of those, parts it could not replace safely (their text shares a
    /// line with a comment), where it went beside the text as a comment.
    commented: usize,
}

/// Comments Gaanim writes beside a part: an earlier dictation, or the text
/// a dictation replaced.
const OWN_COMMENTS: [&str; 3] = ["<!-- dictated:", "<!-- dictado:", "<!-- previous:"];

/// The 1-based lines of `lines` a dictation replaces in part `part` of
/// `section`: those with its text and Gaanim's own comments; the speaker's
/// comments on lines of their own stay. `None` when text and a comment
/// share a line, or a comment spans lines.
fn part_lines(lines: &[&str], section: &ScriptSection, part: usize) -> Option<Vec<usize>> {
    let first = match part.checked_sub(1) {
        Some(previous) => section.marks.get(previous)? + 1,
        None => section.line + 1,
    };
    let last = section
        .marks
        .get(part)
        .map_or(section.end, |mark| mark.saturating_sub(1));
    let mut replaced = Vec::new();
    for line in first..=last {
        let raw = lines.get(line.checked_sub(1)?)?.trim();
        if raw.is_empty() {
            continue;
        }
        let opens = raw.matches("<!--").count();
        let closes = raw.matches("-->").count();
        match (opens, closes) {
            (0, 0) => replaced.push(line),
            (1, 1) if raw.starts_with("<!--") && raw.ends_with("-->") => {
                if OWN_COMMENTS.iter().any(|own| raw.starts_with(own)) {
                    replaced.push(line);
                }
            }
            _ => return None,
        }
    }
    Some(replaced)
}

/// `text` on one line inside an HTML comment, which ends at the first
/// `-->`.
fn comment(label: &str, text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("<!-- {label}: {} -->", text.replace("--", "–"))
}

/// The script `existing` completed (see [`complete_source`]) with the
/// dictated text of each `(slide, part, text)`, which becomes the part's
/// text: in place of what it had, kept below in a `<!-- previous: … -->`
/// comment (one, replacing Gaanim's earlier ones), or before the part's stop
/// mark, or at the end of the section for the text after the last one. A
/// part whose text shares a line with a comment keeps it, and the
/// dictation goes beside it as a comment.
fn merge_dictation(
    existing: Option<&str>,
    slides: &[ScriptSlide],
    dictated: &[(String, usize, String)],
) -> Result<(String, DictationReport), String> {
    let (completed, _) = complete_source(existing, &NarrationManifest::default(), slides)?;
    let sections = parse_script(&completed)?;
    let lines: Vec<&str> = completed.lines().collect();
    let mut report = DictationReport::default();
    // Lines to insert before a 1-based line (one past the last: the end).
    let mut insertions: Vec<(usize, Vec<String>)> = Vec::new();
    // Lines the dictation replaces.
    let mut removed: Vec<usize> = Vec::new();
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
            .map(|written| written.text.trim().to_owned())
            .filter(|written| !written.is_empty());
        let before_mark = match section.marks.get(*part) {
            Some(&mark) => (mark, false),
            None => (section.end + 1, true),
        };
        report.parts += 1;
        match (written, part_lines(&lines, section, *part)) {
            (written, Some(old)) if !old.is_empty() => {
                let mut paragraph = vec![text.to_owned()];
                if let Some(previous) = written {
                    report.replaced += 1;
                    let same = previous.split_whitespace().eq(text.split_whitespace());
                    if !same {
                        paragraph.push(comment("previous", &previous));
                    }
                }
                insertions.push((old[0], paragraph));
                removed.extend(old);
            }
            (Some(_), _) => {
                report.commented += 1;
                let (line, at_end) = before_mark;
                let note = comment("dictated", text);
                insertions.push((
                    line,
                    if at_end {
                        vec![String::new(), note]
                    } else {
                        vec![note, String::new()]
                    },
                ));
            }
            (None, _) => {
                let (line, at_end) = before_mark;
                insertions.push((
                    line,
                    if at_end {
                        vec![String::new(), text.to_owned()]
                    } else {
                        vec![text.to_owned(), String::new()]
                    },
                ));
            }
        }
    }
    let newline = if completed.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output: Vec<String> = Vec::with_capacity(lines.len() + insertions.len() * 2);
    for line in 1..=lines.len() + 1 {
        for (_, inserted) in insertions.iter().filter(|(before, _)| *before == line) {
            output.extend(inserted.iter().cloned());
        }
        if let Some(text) = lines.get(line - 1)
            && !removed.contains(&line)
        {
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
        return Err("The scene has no slides: create segments with scene.segment(...)".to_owned());
    }
    let (token, _) = ScribeToken::resolve(&panel.scribe_token, scribe::vault::load, |name| {
        std::env::var(name).ok()
    })
    .ok_or_else(|| {
        "The Scribe token is missing: paste or save it under Zoom Scribe (dictation) in this \n         panel"
            .to_owned()
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

/// Where recording the talk starts: where the slide named `from` begins,
/// or the first slide when `None`; for its part `part`, at the stop before
/// it.
fn talk_start(timeline: &Timeline, from: Option<&str>, part: Option<usize>) -> Option<f64> {
    let segment = timeline.segments.iter().find(|segment| match from {
        Some(name) => segment.name == name,
        None => segment.name != IMPLICIT_SEGMENT,
    })?;
    match part.and_then(|part| part.checked_sub(1)) {
        Some(previous) => segment.stops.get(previous).map(|stop| stop.time),
        None => Some(crate::presenter::segment_entry_time(timeline, segment)),
    }
}

/// Keep the time of the slides a recording went through whole, for the
/// talk at `source`.
fn record_timing(
    pace: &mut PrompterPace,
    source: Option<&Path>,
    timed: Vec<pace::TimedSlide>,
    panel: &mut NarrationPanel,
) {
    let Some(source) = source.filter(|_| !timed.is_empty()) else {
        return;
    };
    let recorded = chrono::Utc::now().timestamp().max(0) as u64;
    pace.timing = Rehearsals::update_timing(source, |timing| {
        for slide in &timed {
            timing.record(SlideTiming {
                name: slide.name.clone(),
                seconds: slide.seconds,
                parts: slide.parts.clone(),
                recorded,
            });
        }
    });
    pace.source = Some(source.to_path_buf());
    let names: Vec<String> = timed
        .iter()
        .map(|slide| format!("{} {}", slide.name, pace::format_span(slide.seconds, false)))
        .collect();
    panel.notify(NoticeKind::Success, format!("Timed: {}", names.join(" · ")));
}

/// The audio of the parts a recording of the talk timed, to keep.
struct ClipJob {
    /// Where the talk's clips are kept.
    dir: PathBuf,
    /// `(slide, part, start, end)`, in recording seconds.
    clips: Vec<(String, usize, f64, f64)>,
    /// Slides recorded whole, whose earlier clips go.
    whole: Vec<String>,
}

impl ClipJob {
    /// Stop the microphone and write each clip, in the background.
    fn save(self, input: Input) {
        let (samples, rate) = input.recorder.finish();
        std::thread::spawn(move || {
            if let Err(error) = self.write(&samples, rate) {
                gaanim_core::console::error(
                    "narration",
                    format!("the recording's audio was not kept: {error}"),
                );
            }
        });
    }

    fn write(&self, samples: &[f32], rate: u32) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir).map_err(|error| error.to_string())?;
        for slide in &self.whole {
            for path in slide_clips(&self.dir, slide) {
                std::fs::remove_file(path).map_err(|error| error.to_string())?;
            }
        }
        let at = |seconds: f64| ((seconds.max(0.0) * f64::from(rate)) as usize).min(samples.len());
        for (slide, part, start, end) in &self.clips {
            let range = at(*start)..at(*end);
            if range.is_empty() {
                continue;
            }
            write_wav_take(
                &pace::clip_path(&self.dir, slide, *part),
                &samples[range],
                rate,
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

/// The kept audio of the slide named `slide`, part by part, in order.
fn slide_clips(dir: &Path, slide: &str) -> Vec<PathBuf> {
    let prefix = pace::clip_prefix(slide);
    let mut clips: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix) && name.ends_with(".wav"))
        })
        .collect();
    clips.sort();
    clips
}

/// Keep the `seconds` part `part` of the slide named `slide` took, for
/// the talk at `source`, which changes the slide's time.
fn record_part_timing(
    pace: &mut PrompterPace,
    source: Option<&Path>,
    timeline: &Timeline,
    (slide, part): (&str, usize),
    seconds: Option<f64>,
    panel: &mut NarrationPanel,
) {
    let (Some(source), Some(seconds)) = (source, seconds) else {
        return;
    };
    let recorded = chrono::Utc::now().timestamp().max(0) as u64;
    pace.timing = Rehearsals::update_timing(source, |timing| {
        timing.record_part(slide, part, seconds, recorded);
    });
    pace.source = Some(source.to_path_buf());
    let label = timeline
        .segments
        .iter()
        .find(|segment| segment.name == slide)
        .map_or_else(
            || format!("Step {}", part + 1),
            |segment| part_label(segment, part),
        );
    let total = pace
        .timing
        .slide(slide)
        .map(|kept| format!(" (the slide, {})", pace::format_span(kept.seconds, false)))
        .unwrap_or_default();
    panel.notify(
        NoticeKind::Success,
        format!(
            "Timed: {slide} · {label} {}{total}",
            pace::format_span(seconds, false)
        ),
    );
}

/// Write what Scribe dictated into the script and say how it went; keep
/// the speaker's pace it measured for the prompter.
fn write_dictation(
    dictation: Dictation,
    track: &pace::ScriptTrack,
    scope: Option<&Scope>,
    stash: &StashedReplay,
    panel: &mut NarrationPanel,
    pace: &mut PrompterPace,
) {
    let mut parts = scribe::dictated_parts(track, &dictation.turns);
    // Recording one slide or part again writes only there, not what was
    // said while advancing out of it.
    parts.retain(|(slide, part, _)| scope.is_none_or(|scope| scope.covers(slide, *part)));
    if let Some(rate) = scribe::speaking_rate(track, &dictation.turns) {
        let settings = panel.settings();
        let rate = settings
            .speaking_wpm
            .map_or(rate, |kept| (kept + rate) / 2.0);
        settings.speaking_wpm = Some(rate);
        pace.measured = Some(rate);
        if let Err(error) = settings.save() {
            gaanim_core::console::warn("narration", format!("the pace was not saved: {error}"));
        }
    }
    let failure = dictation
        .note
        .as_ref()
        .filter(|(_, failed)| *failed)
        .map(|(message, _)| message.clone());
    drop(dictation);
    if parts.is_empty() {
        let message = match failure {
            Some(failure) => format!("Scribe dictated nothing: {failure}"),
            None => "Scribe did not transcribe any phrase".to_owned(),
        };
        panel.notify(NoticeKind::Error, message);
        return;
    }
    let manifest = manifest(stash);
    let slides = script_slides(stash);
    let Some(path) = script_path(stash, &manifest) else {
        panel.notify(NoticeKind::Error, "the scene has not loaded yet");
        return;
    };
    let existing = match std::fs::read_to_string(&path) {
        Ok(source) => Some(source),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            panel.notify(
                NoticeKind::Error,
                format!("could not read the script: {error}"),
            );
            return;
        }
    };
    let written =
        merge_dictation(existing.as_deref(), &slides, &parts).and_then(|(script, report)| {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            write_atomically(&path, script.as_bytes())
                .map(|()| report)
                .map_err(|error| error.to_string())
        });
    match written {
        Ok(report) => {
            // Counts only: the dictated words stay in the script.
            let mut message = format!(
                "Script dictated: {} part(s) in {}",
                report.parts,
                file_name(&path)
            );
            if report.replaced > 0 {
                message.push_str(&format!(
                    "; it replaced the text of {}, kept below as <!-- previous: … -->",
                    report.replaced
                ));
            }
            if report.commented > 0 {
                message.push_str(&format!(
                    "; {} share a line with a comment, so it went beside them",
                    report.commented
                ));
            }
            if let Some(failure) = failure {
                message.push_str(&format!(". Scribe was cut off: {failure}"));
            }
            panel.notify(NoticeKind::Success, message);
        }
        Err(error) => panel.notify(
            NoticeKind::Error,
            format!("could not write the script: {error}"),
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
    mut pace: ResMut<PrompterPace>,
    mut clip_playback: Option<ResMut<gaanim_media::ClipPlayback>>,
) {
    panel.poll_jobs();
    // A recording does not record what plays back.
    if session.capturing()
        && let Some(playback) = clip_playback.as_deref_mut()
        && playback.playing().is_some()
    {
        playback.stop();
    }
    let pressed = |key| keys.just_pressed(key);
    let reload = reload.map(|reload| reload.0.clone());
    // The talk as last recorded, and the speaker's pace, for the prompter.
    let source = paths.as_ref().map(|paths| paths.script_path.clone());
    if pace.source != source {
        pace.timing = source
            .as_deref()
            .map(|source| Rehearsals::load(source).timing().clone())
            .unwrap_or_default();
        pace.source = source.clone();
    }
    if !pace.loaded {
        pace.measured = panel.settings().speaking_wpm;
        pace.fixed = crate::presenter::prompter::PrompterPreferences::load().fixed();
        pace.loaded = true;
    }
    pace.live = None;

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
                            None => panel
                                .notify(NoticeKind::Error, format!("\"{key}\" has no take yet")),
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
                        Some(path) => match complete_and_open(&path, &manifest, &slides) {
                            Ok((added, opened)) => {
                                let mut message = format!("Script: {}", path.display());
                                let mut news = Vec::new();
                                if added.sections > 0 {
                                    news.push(format!("{} new section(s)", added.sections));
                                }
                                if added.marks > 0 {
                                    news.push(format!("{} [stop] mark(s)", added.marks));
                                }
                                if !news.is_empty() {
                                    message.push_str(&format!(" · {}", news.join(", ")));
                                }
                                if added.from_notes > 0 {
                                    message.push_str(
                                        ". The notes in the code moved to the script, which \
                                         overrides notes=…: you can remove them from the code.",
                                    );
                                }
                                if manifest
                                    .voiceovers
                                    .iter()
                                    .any(|voice| voice.text_source == TextSource::Argument)
                                {
                                    message.push_str(
                                        ". Remove text=… from the code to use the script's text.",
                                    );
                                }
                                if let Err(error) = opened {
                                    message =
                                        format!("{message}. Could not open the editor: {error}");
                                }
                                panel.notify(NoticeKind::Info, message);
                            }
                            Err(error) => panel.notify(
                                NoticeKind::Error,
                                format!("could not prepare the script: {error}"),
                            ),
                        },
                        None => panel.notify(NoticeKind::Error, "the scene has not loaded yet"),
                    }
                    Phase::Idle
                }
                Request::RecordVoiceover(key) => {
                    match manifest.voiceovers.iter().find(|voice| voice.key == key) {
                        Some(_) if panel.busy(&key) => {
                            panel.notify(
                                NoticeKind::Info,
                                format!("wait until the running job on \"{key}\" finishes"),
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
                                format!("\"{key}\" is no longer in the scene"),
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
                Request::Seek(time) => {
                    timeline.seek_request = Some(time);
                    timeline.is_playing = false;
                    Phase::Idle
                }
                Request::RecordTalk {
                    from,
                    part,
                    only,
                    dictate,
                } => {
                    let plan = if dictate {
                        let project = ProjectSpeech::read(paths.as_deref());
                        match dictation_plan(&mut panel, &script_slides(&stash), &project, false) {
                            Ok(plan) => plan,
                            Err(error) => {
                                panel.notify(NoticeKind::Error, error);
                                return;
                            }
                        }
                    } else {
                        CapturePlan {
                            scribe: None,
                            keep_take: false,
                        }
                    };
                    match (Target::live(&manifest), &manifest.live) {
                        // A live take replaces the stops: present the take
                        // with its stops restored, and keep it as it was.
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
                        _ => match talk_start(&timeline, from.as_deref(), part) {
                            Some(start_time) => {
                                let target = Target::Talk {
                                    start_time,
                                    only: from.filter(|_| only).map(|slide| Scope { slide, part }),
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
                            None => {
                                panel.notify(
                                    NoticeKind::Error,
                                    "The scene has no slides: create segments with \
                                     scene.segment(...)",
                                );
                                Phase::Idle
                            }
                        },
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
                    Some("the scene did not reload in time; check the script for errors"),
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
                                    format!("The take records without dictation: {error}"),
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
            track,
            scope,
            since,
        } => {
            dictation.absorb();
            // Esc stops waiting; what arrived still goes into the script.
            if dictation.closed || pressed(KeyCode::Escape) || since.elapsed() > FINISH_WAIT {
                write_dictation(
                    *dictation,
                    &track,
                    scope.as_ref(),
                    &stash,
                    &mut panel,
                    &mut pace,
                );
                Phase::Idle
            } else {
                Phase::Finishing {
                    dictation,
                    track,
                    scope,
                    since,
                }
            }
        }
        Phase::Recording(mut recording) => {
            recording.input.update(Instant::now());
            let elapsed = recording.input.recorder.elapsed();
            if recording.target.presents() {
                recording.follow(&timeline, elapsed);
            }
            if let Some(dictation) = &mut recording.dictation {
                dictation.absorb();
                if dictation.closed && dictation.stopped_at.is_none() {
                    dictation.stopped_at = Some(timeline.current_time);
                }
            }
            pace.live = recording
                .dictation
                .as_ref()
                .and_then(|dictation| scribe::speaking_rate(&recording.track, &dictation.turns));
            if let Some(error) = recording.input.recorder.error() {
                let target = recording.target.clone();
                drop(recording);
                finish_capture(&target, &mut timeline, &mut preview_audio);
                if target.is_live() {
                    abort_live(&mut panel, reload.as_deref(), None);
                }
                panel.notify(NoticeKind::Error, format!("the microphone failed: {error}"));
                return;
            }
            if pressed(KeyCode::Escape) {
                let target = recording.target.clone();
                let message = if recording.keep_take {
                    "Recording discarded"
                } else {
                    "Dictation discarded"
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
            // Recording one slide or part again ends once the speaker
            // leaves it.
            let scope = match &recording.target {
                Target::Talk { only, .. } => only.clone(),
                _ => None,
            };
            let left_only = scope
                .as_ref()
                .is_some_and(|scope| left_scope(&recording.track, scope));
            if left_only || pressed(KeyCode::Enter) || pressed(KeyCode::NumpadEnter) {
                if let Some(stop_time) = recording.waiting_at.take() {
                    let hold = live_hold(
                        elapsed,
                        stop_time,
                        recording.target.start_time(),
                        &recording.holds,
                    );
                    recording.holds.push(hold);
                }
                let position = ScriptPosition::of(&timeline);
                let at_end = timeline.current_time >= timeline.cached_duration - 1e-4;
                // The audio of the parts timed, when the speaker keeps it.
                let mut clips = (panel.settings().keep_talk_audio && !recording.keep_take)
                    .then(|| source.as_deref().and_then(pace::Rehearsals::clips_dir))
                    .flatten()
                    .map(|dir| ClipJob {
                        dir,
                        clips: Vec::new(),
                        whole: Vec::new(),
                    });
                match scope
                    .as_ref()
                    .and_then(|scope| Some((&scope.slide, scope.part?)))
                {
                    // One part: it counts once left, or finished resting at
                    // its stop.
                    Some((slide, part)) => {
                        let finished = at_end
                            || (!left_only
                                && position.is_some_and(|position| {
                                    position.resting
                                        && position.part == part
                                        && timeline.segments[position.slide].name == *slide
                                }));
                        let span =
                            pace::timed_part(&recording.track, slide, part, elapsed, finished);
                        if let (Some(job), Some((start, end))) = (&mut clips, span) {
                            job.clips.push((slide.clone(), part, start, end));
                        }
                        record_part_timing(
                            &mut pace,
                            source.as_deref(),
                            &timeline,
                            (slide, part),
                            span.map(|(start, end)| end - start),
                            &mut panel,
                        );
                    }
                    None if recording.target.presents() => {
                        // The slide being said counts once all of it is said.
                        let finished_last = at_end
                            || (!left_only
                                && position.is_some_and(|position| {
                                    position.finishes_slide(&timeline.segments)
                                }));
                        let timed = pace::timed_slides(
                            &recording.track,
                            elapsed,
                            recording.entered_first,
                            finished_last,
                        );
                        if let Some(job) = &mut clips {
                            let names: Vec<&str> =
                                timed.iter().map(|slide| slide.name.as_str()).collect();
                            job.clips = pace::part_spans(&recording.track, elapsed, &names);
                            job.whole = timed.iter().map(|slide| slide.name.clone()).collect();
                        }
                        record_timing(&mut pace, source.as_deref(), timed, &mut panel);
                    }
                    None => {}
                }
                let track = std::mem::take(&mut recording.track);
                let dictation = recording.dictation.take();
                if recording.keep_take {
                    save_recording(*recording, &mut timeline, &mut panel, &mut preview_audio);
                } else {
                    let target = recording.target.clone();
                    // Where the recording reached: the slide it went on to
                    // after the one recorded again, else where Scribe
                    // stopped, if it stopped early, so what it missed can be
                    // dictated again.
                    let reached = if left_only {
                        match scope
                            .as_ref()
                            .and_then(|scope| Some((&scope.slide, scope.part?)))
                        {
                            // Resting at the stop of the part recorded again.
                            Some((slide, part)) => timeline
                                .segments
                                .iter()
                                .find(|segment| segment.name == *slide)
                                .and_then(|segment| segment.stops.get(part))
                                .map(|stop| stop.time),
                            None => position.map(|position| {
                                crate::presenter::segment_entry_time(
                                    &timeline,
                                    &timeline.segments[position.slide],
                                )
                            }),
                        }
                    } else {
                        dictation
                            .as_ref()
                            .and_then(|dictation| dictation.stopped_at)
                    }
                    .unwrap_or(timeline.current_time);
                    match clips.filter(|job| !job.clips.is_empty()) {
                        Some(job) => {
                            let Recording { input, .. } = *recording;
                            job.save(input);
                            panel.clip_files = None;
                        }
                        None => drop(recording),
                    }
                    finish_capture(&target, &mut timeline, &mut preview_audio);
                    if target.is_live() {
                        // Back to the take as it was recorded.
                        abort_live(&mut panel, reload.as_deref(), None);
                    } else {
                        // The timeline stays where the dictation reached, and
                        // the next one goes on from there.
                        timeline.seek_request = Some(reached);
                    }
                }
                if let Some(dictation) = dictation {
                    dictation.session.finish();
                    session.phase = Phase::Finishing {
                        dictation: Box::new(dictation),
                        track,
                        scope,
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
                Target::Live { start_time, .. } | Target::Talk { start_time, .. } => {
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
                "wait until the running job on \"{}\" finishes",
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
        panel.notify(NoticeKind::Error, "the scene cannot be reloaded");
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
    let entered_first = timeline.segments.iter().any(|segment| {
        (crate::presenter::segment_entry_time(timeline, segment) - start).abs() < 1e-4
    });
    // Recording one part again plays on from the stop before it.
    let rolls_on = matches!(
        &target,
        Target::Talk {
            only: Some(Scope {
                part: Some(part),
                ..
            }),
            ..
        } if *part > 0
    );
    // A presentation that starts on a stop waits there first.
    let waiting_at = (target.presents()
        && !rolls_on
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
        track: Vec::new(),
        entered_first,
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
        panel.notify(NoticeKind::Error, "The take is too short; it was not saved");
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
        Target::Live { .. } | Target::Talk { .. } => TakeSidecar {
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
            format!("could not save the take: {error}"),
        );
        return;
    }
    if clipped > 0 {
        panel.notify(
            NoticeKind::Error,
            format!(
                "Take \"{key}\" saved, but it clipped {clipped} time(s): better redo it \
                 with less gain"
            ),
        );
    } else {
        panel.notify(
            NoticeKind::Success,
            format!("Take \"{key}\" saved · {}", format_seconds(seconds)),
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
        MarkerSource::Tapped => "tapped",
        MarkerSource::Transcript => "Whisper",
        MarkerSource::Estimated => "estimated",
        MarkerSource::Missing => "not found",
    }
}

fn text_source_label(source: TextSource) -> &'static str {
    match source {
        TextSource::Argument => "text in the code",
        TextSource::Script => "text from the script",
        TextSource::Notes => "text from the segment's notes",
        TextSource::Missing => "no text",
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
    response.on_hover_text("Drag to move");
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
    timeline: Res<Timeline>,
    pace: Res<PrompterPace>,
    mut clip_playback: Option<ResMut<gaanim_media::ClipPlayback>>,
    mut settings_window: ResMut<crate::settings::SettingsWindow>,
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
    let mut close = false;
    let mut request = None;
    let mut settings_changed = false;
    let mut open_settings = false;
    let screen = ctx.viewport_rect();
    let max_height = (screen.height() - 120.0).max(240.0);
    let can_dictate = panel.token_available();
    let dictating = panel.dictating() && can_dictate;
    let mut dictate = dictating;
    let mut expanded = std::mem::take(&mut panel.expanded);
    let clips_dir = paths
        .as_ref()
        .and_then(|paths| pace::Rehearsals::clips_dir(&paths.script_path));
    let clip_files = panel.clip_files(clips_dir.as_deref());
    let keeping = panel.settings().keep_talk_audio;
    let mut keep_audio = keeping;

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
                    ui.set_width(PANEL_WIDTH);
                    enlarge(ui.style_mut());
                    ui.horizontal(|ui| {
                        drag_grip(ui);
                        paint_mic_badge(ui);
                        ui.label(
                            egui::RichText::new("Script & narration")
                                .size(18.0)
                                .strong()
                                .color(palette::TEXT),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if icon_button(ui, Icon::Close, ButtonTone::Ghost, true)
                                .on_hover_text("Close")
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
                            if !slides.is_empty() {
                                ui.label(heading("TIMING"));
                                let mut audio = TalkAudio {
                                    keep: &mut keep_audio,
                                    dir: clips_dir.as_deref(),
                                    files: &clip_files,
                                    playback: clip_playback.as_deref_mut(),
                                };
                                timing_card(
                                    ui,
                                    &timeline,
                                    &pace.timing,
                                    manifest.live.is_some(),
                                    &mut expanded,
                                    &mut dictate,
                                    can_dictate,
                                    &mut audio,
                                    &mut request,
                                );
                            }
                            if let Some(live) = &manifest.live {
                                ui.label(heading("LIVE TAKE"));
                                let busy = panel.busy(&live.key);
                                live_card(ui, live, busy, &mut dictate, &mut request);
                            }
                            if !manifest.voiceovers.is_empty() {
                                ui.label(heading("VOICEOVER"));
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
                            if small(ui, "Recording, transcription and dictation settings…", true)
                                .on_hover_text("Opens Settings · Ctrl+,")
                                .clicked()
                            {
                                open_settings = true;
                            }
                        });

                    if let Some((message, kind)) = &panel.notice {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(message)
                                    .size(14.0)
                                    .color(notice_color(*kind)),
                            )
                            .wrap(),
                        );
                    }
                });
        });

    if dictate != dictating {
        panel.dictate = Some(dictate);
    }
    panel.expanded = expanded;
    if keep_audio != keeping {
        panel.settings().keep_talk_audio = keep_audio;
        settings_changed = true;
    }
    if settings_changed && let Err(error) = panel.settings().save() {
        panel.notify(
            NoticeKind::Error,
            format!("the settings were not saved: {error}"),
        );
    }
    if close {
        panel.open = false;
    }
    if let Some(request) = request {
        panel.request = Some(request);
    }
    if open_settings {
        settings_window.show(crate::settings::Section::Recording);
    }
}

/// Carry out what the Scribe settings asked for, and say how it went.
fn apply_vault_action(panel: &mut NarrationPanel, project: &ProjectSpeech, action: VaultAction) {
    match action {
        VaultAction::Save => {
            let saved = ScribeToken::new(&panel.scribe_token)
                .ok_or_else(|| "paste the token before saving it".to_owned())
                .and_then(|token| scribe::vault::save(&token));
            match saved {
                Ok(()) => {
                    panel.scribe_token.clear();
                    panel.token_stored = Some(true);
                    panel.notify(
                        NoticeKind::Success,
                        "Scribe token saved in Windows Credential Manager",
                    );
                }
                Err(error) => panel.notify(NoticeKind::Error, error),
            }
        }
        VaultAction::Forget => match scribe::vault::forget() {
            Ok(()) => {
                panel.token_stored = Some(false);
                panel.notify(
                    NoticeKind::Info,
                    "Scribe token removed from the credential store",
                );
            }
            Err(error) => panel.notify(NoticeKind::Error, error),
        },
        VaultAction::Pin(language) => {
            let pinned = language.clone();
            write_project_language(panel, project, Some(language), move || {
                format!("Language pinned in gaanim.toml: language = \"{pinned}\"")
            });
        }
        VaultAction::Unpin => {
            write_project_language(panel, project, None, || {
                "gaanim.toml no longer pins the language: yours is used".to_owned()
            });
        }
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
            "this scene does not belong to a project with a gaanim.toml",
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
            "The scene declares no narration. Add a voiceover block \
             and save the script:",
        )
        .size(14.5)
        .color(palette::TEXT_MUTED),
    );
    field_frame().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(
            egui::RichText::new(
                "with scene.voiceover(\"intro\") as vo:\n    \
                 scene.play([title.animate.write()])\n    vo.wait_until(\"derivative\")",
            )
            .monospace()
            .size(13.5)
            .color(palette::TEXT),
        );
    });
    ui.label(
        egui::RichText::new(
            "Each block's text goes in the script (## intro). Or record while presenting: \
             call scene.live_take(\"lecture\") at the start and use scene.stop() where \
             you want to pause.",
        )
        .size(14.5)
        .color(palette::TEXT_MUTED),
    );
}

fn slides_hint(ui: &mut egui::Ui) {
    ui.label(
        egui::RichText::new(
            "The talk's script: one \"## name\" section per slide with what you will \
             say, and a [stop] line where you advance. Presenter View shows it in its \
             notes and, with T, as a teleprompter. Gaanim adds the missing slides and \
             stops without touching your text.",
        )
        .size(14.5)
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
                egui::RichText::new("Script")
                    .size(15.0)
                    .strong()
                    .color(palette::TEXT),
            );
            let detail = match (&manifest.script, path) {
                (Some(script), _) => format!(
                    "{} · {} sections",
                    file_name(&script.path),
                    script.sections.len()
                ),
                (None, Some(path)) => format!("not created · {}", file_name(path)),
                (None, None) => "not created".to_string(),
            };
            ui.label(
                egui::RichText::new(detail)
                    .size(14.0)
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
            (None, _) => "Create script".to_string(),
            (Some(_), 0) => "Open script".to_string(),
            (Some(_), missing) => format!("Open and complete ({missing})"),
        };
        if secondary(ui, &label, path.is_some())
            .on_hover_text(
                "Write under \"## name\" what you say on each slide or voiceover block, \
                 with [stop] where you advance; when you save, the scene updates",
            )
            .clicked()
        {
            *request = Some(Request::OpenScript);
        }
    });
}

/// The kept audio of the talk, as the timing card shows and plays it.
struct TalkAudio<'a> {
    /// Keep the audio of each recording.
    keep: &'a mut bool,
    /// Where it is kept, and the names of the files there.
    dir: Option<&'a Path>,
    files: &'a [String],
    playback: Option<&'a mut gaanim_media::ClipPlayback>,
}

impl TalkAudio<'_> {
    /// The kept audio of part `part` of the slide named `slide`, or of all
    /// its parts.
    fn clips(&self, slide: &str, part: Option<usize>) -> Vec<PathBuf> {
        let Some(dir) = self.dir else {
            return Vec::new();
        };
        let prefix = pace::clip_prefix(slide);
        let name = part.map(|part| format!("{prefix}{part:02}.wav"));
        self.files
            .iter()
            .filter(|file| match &name {
                Some(name) => *file == name,
                None => file.starts_with(&prefix),
            })
            .map(|file| dir.join(file))
            .collect()
    }

    /// A button that plays `clips`, named `label`, or stops them.
    fn listen(&mut self, ui: &mut egui::Ui, label: String, clips: Vec<PathBuf>) {
        let playing = self
            .playback
            .as_deref()
            .is_some_and(|playback| playback.playing() == Some(label.as_str()));
        let icon = if playing { Icon::Pause } else { Icon::Play };
        let enabled = !clips.is_empty() && self.playback.is_some();
        let hint = match (playing, enabled) {
            (true, _) => "Stop",
            (false, true) => "Listen to what you recorded",
            (false, false) => "Nothing recorded with Keep the audio yet",
        };
        if icon_button(ui, icon, ButtonTone::Ghost, enabled)
            .on_hover_text(hint)
            .clicked()
            && let Some(playback) = self.playback.as_deref_mut()
        {
            if playing {
                playback.stop();
            } else {
                playback.play(label, clips);
            }
        }
    }
}

/// The talk's timing: the time each slide took when last recorded against
/// its target, and each of its stops once unfolded; the total against the
/// planned length; buttons to record the talk, or one slide or stop again,
/// and to listen to what was recorded. Clicking a name moves the playhead
/// there.
#[allow(clippy::too_many_arguments)]
fn timing_card(
    ui: &mut egui::Ui,
    timeline: &Timeline,
    timing: &pace::TalkTiming,
    live: bool,
    expanded: &mut std::collections::BTreeSet<String>,
    dictate: &mut bool,
    can_dictate: bool,
    audio: &mut TalkAudio,
    request: &mut Option<Request>,
) {
    let slides: Vec<_> = timeline
        .segments
        .iter()
        .filter(|segment| segment.name != IMPLICIT_SEGMENT)
        .collect();
    let planned = planned_length(timeline);
    let recorded: Vec<Option<&SlideTiming>> = slides
        .iter()
        .map(|slide| timing.slide(&slide.name))
        .collect();
    let total: f64 = recorded.iter().flatten().map(|slide| slide.seconds).sum();
    let missing = recorded.iter().filter(|slide| slide.is_none()).count();
    let dictating = *dictate && can_dictate;
    let record = |slide: &str, part: Option<usize>, only: bool| Request::RecordTalk {
        from: Some(slide.to_owned()),
        part,
        only,
        dictate: dictating,
    };
    let live_hint = "A live take is recorded whole: record it again from its card";
    field_frame()
        .inner_margin(egui::Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Recorded")
                        .size(15.0)
                        .strong()
                        .color(palette::TEXT),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let mut summary = pace::format_span(total, false);
                    if let Some(planned) = planned {
                        summary.push_str(&format!(" of {}", pace::format_span(planned, false)));
                    }
                    let color = if missing == 0 {
                        timing_color(total, planned)
                    } else {
                        palette::TEXT_MUTED
                    };
                    ui.label(
                        egui::RichText::new(summary)
                            .size(15.0)
                            .monospace()
                            .color(color),
                    )
                    .on_hover_text(match planned {
                        Some(_) => {
                            "The slides' recorded times against the talk's planned length \
                             (scene.slides.target, or every slide's target)"
                        }
                        None => {
                            "Plan the talk with scene.slides.target(...) or a target=… on \
                             every slide to see how it fits"
                        }
                    });
                });
            });
            ui.add_space(4.0);
            // The names take what the times and buttons leave.
            let name_width = (ui.available_width() - 270.0).max(120.0);
            egui::Grid::new("talk-timing")
                .num_columns(4)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    for (slide, kept) in slides.iter().zip(&recorded) {
                        let parts = slide_parts(slide);
                        let open = expanded.contains(&slide.name);
                        ui.horizontal(|ui| {
                            ui.set_width(name_width);
                            ui.spacing_mut().item_spacing.x = 6.0;
                            if parts > 1 {
                                let toggle = disclosure(ui, open).on_hover_text(if open {
                                    "Hide its stops"
                                } else {
                                    "Show its stops, to record or hear one again"
                                });
                                if toggle.clicked() {
                                    if open {
                                        expanded.remove(&slide.name);
                                    } else {
                                        expanded.insert(slide.name.clone());
                                    }
                                }
                            } else {
                                ui.add_space(16.0);
                            }
                            if name_link(ui, &slide.name, 15.0, palette::TEXT)
                                .on_hover_text(format!("{} · go to this slide", slide.name))
                                .clicked()
                            {
                                *request = Some(Request::Seek(
                                    crate::presenter::segment_entry_time(timeline, slide),
                                ));
                            }
                        });
                        let seconds = kept.map(|kept| kept.seconds);
                        timing_cell(ui, seconds, slide.target, 15.0);
                        let detail = match (seconds, slide.target) {
                            (Some(seconds), Some(target)) if seconds > target + 0.5 => {
                                format!("{} over", pace::format_span(seconds - target, true))
                            }
                            (_, Some(target)) => {
                                format!("of {}", pace::format_span(target, false))
                            }
                            _ => String::new(),
                        };
                        ui.label(
                            egui::RichText::new(detail)
                                .size(13.5)
                                .color(palette::TEXT_FAINT),
                        );
                        let (label, hint, only) = if kept.is_some() {
                            (
                                "Re-record",
                                "Present only this slide again: it ends once you leave it, \
                                 and its time, script and audio are replaced",
                                true,
                            )
                        } else {
                            (
                                "Record",
                                "Present from this slide on: each slide you go through keeps \
                                 its time; Enter finishes",
                                false,
                            )
                        };
                        ui.horizontal(|ui| {
                            let clips = audio.clips(&slide.name, None);
                            audio.listen(ui, slide.name.clone(), clips);
                            if small(ui, label, !live)
                                .on_hover_text(if live { live_hint } else { hint })
                                .clicked()
                            {
                                *request = Some(record(&slide.name, None, only));
                            }
                        });
                        ui.end_row();
                        if !open {
                            continue;
                        }
                        for part in 0..parts {
                            let label = part_label(slide, part);
                            ui.horizontal(|ui| {
                                ui.set_width(name_width);
                                ui.add_space(22.0);
                                let time = match slide.stops.get(part) {
                                    Some(stop) => stop.time,
                                    // Past the last stop.
                                    None => slide
                                        .stops
                                        .last()
                                        .map_or(slide.start_time, |stop| stop.time + 1e-3),
                                };
                                if name_link(ui, &label, 14.0, palette::TEXT_MUTED)
                                    .on_hover_text("Go to this stop")
                                    .clicked()
                                {
                                    *request = Some(Request::Seek(time));
                                }
                            });
                            let seconds = kept.and_then(|kept| kept.parts.get(part).copied());
                            timing_cell(ui, seconds, None, 14.0);
                            ui.label("");
                            ui.horizontal(|ui| {
                                let clips = audio.clips(&slide.name, Some(part));
                                audio.listen(ui, format!("{} · {label}", slide.name), clips);
                                if small(ui, "Re-record", !live)
                                    .on_hover_text(if live {
                                        live_hint
                                    } else {
                                        "Present only this stop again: from the stop before \
                                         it until you advance at this one. Its script and \
                                         audio are replaced, and its time too once the slide \
                                         has one"
                                    })
                                    .clicked()
                                {
                                    *request = Some(record(&slide.name, Some(part), true));
                                }
                            });
                            ui.end_row();
                        }
                    }
                });
            if let Some((verdict, color)) = timing_verdict(total, missing, planned) {
                ui.add(
                    egui::Label::new(egui::RichText::new(verdict).size(14.0).color(color)).wrap(),
                );
            }
            ui.add_space(2.0);
            if primary(ui, "Record the talk", Some(Icon::Record), true)
                .on_hover_text(
                    "Present from the first slide while speaking: each slide you go through \
                     keeps its time. → or Space advances at each stop, Enter finishes, Esc \
                     discards",
                )
                .clicked()
            {
                *request = Some(Request::RecordTalk {
                    from: None,
                    part: None,
                    only: false,
                    dictate: dictating,
                });
            }
            ui.add_enabled(
                can_dictate,
                egui::Checkbox::new(dictate, "Write what I say into the script (Zoom Scribe)"),
            )
            .on_hover_text(
                "Each phrase goes to its slide and stop and replaces the text there; the \
                 earlier text stays below as a <!-- previous: … --> comment",
            )
            .on_disabled_hover_text(
                "Paste or save a Scribe token under Zoom Scribe (dictation) below to dictate",
            );
            crate::ui_kit::checkbox(ui, audio.keep, "Keep the audio, to listen back")
                .on_hover_text(
                    "Each recording keeps the audio of every slide and stop it times, in \
                     Gaanim's data folder on this computer, never in the project; recording \
                     one again replaces its audio",
                );
        });
}

/// A triangle that folds (pointing right) or unfolds (pointing down) a
/// list.
fn disclosure(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(16.0), egui::Sense::click());
    let color = if response.hovered() {
        palette::TEXT
    } else {
        palette::TEXT_MUTED
    };
    let center = rect.center();
    let side = 4.5;
    let points = if open {
        vec![
            egui::pos2(center.x - side, center.y - side * 0.6),
            egui::pos2(center.x + side, center.y - side * 0.6),
            egui::pos2(center.x, center.y + side * 0.7),
        ]
    } else {
        vec![
            egui::pos2(center.x - side * 0.6, center.y - side),
            egui::pos2(center.x + side * 0.7, center.y),
            egui::pos2(center.x - side * 0.6, center.y + side),
        ]
    };
    ui.painter().add(egui::Shape::convex_polygon(
        points,
        color,
        egui::Stroke::NONE,
    ));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A name that moves the playhead when clicked.
fn name_link(ui: &mut egui::Ui, text: &str, size: f32, color: egui::Color32) -> egui::Response {
    ui.add(
        egui::Label::new(egui::RichText::new(text).size(size).color(color))
            .truncate()
            .sense(egui::Sense::click()),
    )
    .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A recorded time against its target, or a dash without one.
fn timing_cell(ui: &mut egui::Ui, seconds: Option<f64>, target: Option<f64>, size: f32) {
    match seconds {
        Some(seconds) => ui.label(
            egui::RichText::new(pace::format_span(seconds, false))
                .monospace()
                .size(size)
                .color(timing_color(seconds, target)),
        ),
        None => ui.label(
            egui::RichText::new("—")
                .size(size)
                .color(palette::TEXT_FAINT),
        ),
    };
}

/// The talk's planned length: `scene.slides.target`, else every slide's
/// target added up when each has one.
fn planned_length(timeline: &Timeline) -> Option<f64> {
    timeline.talk_target.or_else(|| {
        let mut slides = timeline
            .segments
            .iter()
            .filter(|segment| segment.name != IMPLICIT_SEGMENT)
            .peekable();
        slides.peek()?;
        slides.map(|slide| slide.target).sum()
    })
}

/// The color of `seconds` against `target`: within it, a little over (a
/// tenth, at least ten seconds), or further over.
fn timing_color(seconds: f64, target: Option<f64>) -> egui::Color32 {
    match target {
        Some(target) if seconds <= target + 0.5 => palette::LOOP,
        Some(target) if seconds <= target + (target * 0.1).max(10.0) => palette::STOP,
        Some(_) => palette::DANGER,
        None => palette::TEXT_MUTED,
    }
}

/// What the recorded times say about the plan of `planned` seconds, with
/// `missing` slides still without a time, and its color.
fn timing_verdict(
    total: f64,
    missing: usize,
    planned: Option<f64>,
) -> Option<(String, egui::Color32)> {
    let planned = planned?;
    let left = planned - total;
    if missing > 0 {
        return Some(if left > 0.0 {
            (
                format!(
                    "{} left for the {missing} slide(s) without a time",
                    pace::format_span(left, false)
                ),
                palette::TEXT_MUTED,
            )
        } else {
            (
                format!(
                    "Already {} over the plan, with {missing} slide(s) to go",
                    pace::format_span(-left, false)
                ),
                palette::DANGER,
            )
        });
    }
    Some(if left < -0.5 {
        (
            format!(
                "{} over the plan: record the slides in amber or red again, saying less",
                pace::format_span(-left, false)
            ),
            timing_color(total, Some(planned)),
        )
    } else {
        (
            format!(
                "Fits the plan with {} to spare",
                pace::format_span(left.max(0.0), false)
            ),
            palette::LOOP,
        )
    })
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
                .size(14.0)
                .color(palette::TEXT_MUTED),
        );
    });
}

/// Final voice loudness a take was leveled to, or a note that it was not.
fn loudness_detail(loudness: Option<f64>) -> String {
    match loudness {
        Some(loudness) => format!(" · {loudness:.0} LUFS"),
        None => " · not leveled".to_string(),
    }
}

/// Final loudness far from the recommendation, and why.
fn loudness_warning(loudness: f64) -> Option<(egui::Color32, &'static str)> {
    if loudness > settings::LOUDNESS_RANGE.1 {
        Some((
            palette::DANGER,
            "Louder than YouTube's level (-14 LUFS): the platform will turn it down and \
             the peak limiter works harder.",
        ))
    } else if loudness < settings::LOUDNESS_RANGE.0 {
        Some((
            palette::STOP,
            "Below -20 LUFS the voice will sound weak next to other videos.",
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
                .size(16.0)
                .strong()
                .color(palette::TEXT),
        );
        let stops = match live.stops {
            1 => "1 stop".to_string(),
            count => format!("{count} stops"),
        };
        let detail = match live.duration {
            Some(duration) if live.applied => format!(
                "Recorded · {} · {stops}{}",
                format_seconds(duration),
                loudness_detail(live.loudness)
            ),
            _ => format!("Not recorded · {stops} in the scene"),
        };
        status_line(ui, live.applied, detail);
        ui.label(
            egui::RichText::new(
                "Present the scene while speaking: → or Space continues at each stop, \
                 Enter finishes. The teleprompter shows each slide's script \
                 and marks where to advance.",
            )
            .size(13.5)
            .color(palette::TEXT_FAINT),
        );
        ui.horizontal_wrapped(|ui| {
            let label = if live.applied {
                "Re-record take"
            } else {
                "Record live take"
            };
            if primary(ui, label, Some(Icon::Record), !busy).clicked() {
                *request = Some(Request::RecordLive { dictate: *dictate });
            }
            if live.applied {
                if secondary(ui, "Listen", true).clicked() {
                    *request = Some(Request::Listen(live.start_time));
                }
                if small(ui, if busy { "Leveling…" } else { "Level" }, !busy)
                    .on_hover_text("Brings the voice to the level set for video")
                    .clicked()
                {
                    *request = Some(Request::Level(live.key.clone()));
                }
            }
        });
        crate::ui_kit::checkbox(ui, dictate, "Also dictate the script with Scribe").on_hover_text(
            "While you record, Zoom Scribe transcribes your voice and each phrase goes to \
                 its slide in the script (set it up under Zoom Scribe (dictation) below)",
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
                    .size(16.0)
                    .strong()
                    .color(palette::TEXT),
            );
            if voice.segment != "_default" {
                ui.label(
                    egui::RichText::new(&voice.segment)
                        .size(14.0)
                        .color(palette::TEXT_FAINT),
                );
            }
        });
        let detail = if voice.recorded {
            let transcript = if voice.transcribed {
                " · with transcript"
            } else {
                ""
            };
            format!(
                "Recorded · {}{}{transcript}",
                format_seconds(voice.duration),
                loudness_detail(voice.loudness)
            )
        } else {
            format!(
                "Not recorded · ~{} estimated",
                format_seconds(voice.duration)
            )
        };
        status_line(ui, voice.recorded, detail);
        let text_color = if voice.text_source == TextSource::Missing {
            palette::STOP
        } else {
            palette::TEXT_FAINT
        };
        ui.label(
            egui::RichText::new(text_source_label(voice.text_source))
                .size(13.5)
                .color(text_color),
        );
        if !voice.markers.is_empty() {
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                for marker in &voice.markers {
                    let color = source_color(marker.source);
                    let text = egui::RichText::new(&marker.name).size(13.5).color(color);
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
            let label = if voice.recorded {
                "Re-record"
            } else {
                "Record"
            };
            if primary(ui, label, Some(Icon::Record), !busy).clicked() {
                *request = Some(Request::RecordVoiceover(voice.key.clone()));
            }
            if secondary(ui, "Listen", true).clicked() {
                *request = Some(Request::Listen(voice.start_time));
            }
            if voice.recorded {
                let label = if transcribing {
                    "Transcribing…"
                } else {
                    "Transcribe"
                };
                if small(ui, label, !busy)
                    .on_hover_text("Finds the markers by word with Whisper")
                    .clicked()
                {
                    *request = Some(Request::Transcribe(voice.key.clone()));
                }
                let label = if leveling { "Leveling…" } else { "Level" };
                if small(ui, label, !busy)
                    .on_hover_text("Brings the voice to the level set for video")
                    .clicked()
                {
                    *request = Some(Request::Level(voice.key.clone()));
                }
            }
        });
    });
}

/// Recorder, voice level and Whisper preferences. Returns whether they changed.
/// The recording preferences, in Settings.
fn recording_form(ui: &mut egui::Ui, settings: &mut NarrationSettings) -> bool {
    let before = settings.clone();
    {
        crate::ui_kit::checkbox(
            ui,
            &mut settings.level_voice,
            "Level each new take when saving it",
        );
        ui.horizontal(|ui| {
            section_label(ui, "Final voice level");
            ui.add_enabled(
                settings.level_voice,
                egui::DragValue::new(&mut settings.loudness)
                    .range(-24.0..=-10.0)
                    .speed(0.1)
                    .fixed_decimals(1)
                    .suffix(" LUFS"),
            );
            if (settings.loudness - settings::DEFAULT_LOUDNESS).abs() > 0.05
                && small(ui, "Recommended", settings.level_voice)
                    .on_hover_text("Back to -16 LUFS")
                    .clicked()
            {
                settings.loudness = settings::DEFAULT_LOUDNESS;
            }
        });
        ui.label(
            egui::RichText::new(format!(
                "Recommended: {:.0} LUFS, a clear voice in online video. YouTube plays \
                 at -14 and television uses -23. Peak ceiling -1.5 dBTP; the original is \
                 kept in narration/.originals.",
                settings::DEFAULT_LOUDNESS
            ))
            .size(13.5)
            .color(palette::TEXT_FAINT),
        );
        if settings.level_voice
            && let Some((color, warning)) = loudness_warning(settings.loudness)
        {
            ui.label(egui::RichText::new(warning).size(13.5).color(color));
        }
        ui.label(
            egui::RichText::new(format!(
                "While recording, peaks should stay between {:.0} and {:.0} dBFS; the meter \
                 warns in red if you get close to 0 dBFS.",
                IDEAL_PEAK_DB.0, IDEAL_PEAK_DB.1
            ))
            .size(13.5)
            .color(palette::TEXT_FAINT),
        );
        crate::ui_kit::checkbox(
            ui,
            &mut settings.countdown,
            "Countdown of 3 s before recording",
        );
        crate::ui_kit::checkbox(
            ui,
            &mut settings.keep_talk_audio,
            "Keep the audio of each recording of the talk, to listen back to it",
        );
    }
    *settings != before
}

/// The whisper.cpp preferences, in Settings.
fn whisper_form(ui: &mut egui::Ui, settings: &mut NarrationSettings) -> bool {
    let before = settings.clone();
    {
        ui.label(
            egui::RichText::new(
                "whisper.cpp finds the markers by word so you do not have to \
                 press Space while recording. The marker must be named like the word \
                 you say.",
            )
            .size(13.5)
            .color(palette::TEXT_FAINT),
        );
        section_label(ui, "whisper-cli (empty: search the PATH)");
        path_field(ui, &mut settings.whisper_cli, "whisper-cli", &["exe"]);
        section_label(ui, "ggml model (e.g. ggml-base.bin)");
        path_field(ui, &mut settings.whisper_model, "Model", &["bin"]);
        ui.horizontal(|ui| {
            section_label(ui, "Language");
            ui.add(
                egui::TextEdit::singleline(&mut settings.language)
                    .desired_width(60.0)
                    .font(egui::FontId::monospace(12.0)),
            );
        });
        crate::ui_kit::checkbox(
            ui,
            &mut settings.auto_transcribe,
            "Transcribe each new take when saving it",
        );
        let status = match settings.whisper() {
            Ok((cli, _)) => (palette::LOOP, format!("Ready · {}", cli.display())),
            Err(error) => (palette::TEXT_FAINT, error),
        };
        ui.add(egui::Label::new(egui::RichText::new(status.1).size(13.5).color(status.0)).wrap());
    }
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
    {
        let faint = |text: String| {
            egui::RichText::new(text)
                .size(13.5)
                .color(palette::TEXT_FAINT)
        };
        ui.add(
            egui::Label::new(faint(
                "Dictate the script while presenting: Zoom's Scribe Live transcribes your \
                 voice and each phrase goes to the slide and part you were presenting."
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
            Some(TokenSource::Typed) => (palette::LOOP, "Token pasted for this session".to_owned()),
            Some(TokenSource::Vault) => (
                palette::LOOP,
                "Token saved in Windows Credential Manager".to_owned(),
            ),
            Some(TokenSource::Environment) => (
                palette::LOOP,
                format!("Token from the {} variable", scribe::TOKEN_VARIABLE),
            ),
            None => (palette::TEXT_FAINT, "No token: paste it here".to_owned()),
        };
        ui.label(egui::RichText::new(status).size(13.5).color(color));
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(typed_token)
                    .password(true)
                    .hint_text("Paste token")
                    .desired_width(180.0),
            );
            if !typed_token.is_empty() {
                if scribe::vault::AVAILABLE
                    && small(ui, "Save in Windows", true)
                        .on_hover_text(
                            "Windows Credential Manager encrypts it for your user; it never \
                             goes to the project folder",
                        )
                        .clicked()
                {
                    action = Some(VaultAction::Save);
                }
                if small(ui, "Forget", true)
                    .on_hover_text("Removes it from this session")
                    .clicked()
                {
                    typed_token.clear();
                }
            } else if stored
                && small(ui, "Remove saved token", true)
                    .on_hover_text("Deletes it from Windows Credential Manager")
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
            section_label(ui, "Language");
            egui::ComboBox::from_id_salt("narration_scribe_language")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    for language in scribe::LANGUAGES {
                        ui.selectable_value(&mut chosen, language, language);
                    }
                });
            if pinned {
                if small(ui, "Unpin", true)
                    .on_hover_text("Removes language from gaanim.toml and uses your language")
                    .clicked()
                {
                    action = Some(VaultAction::Unpin);
                }
            } else if project.manifest.is_some()
                && small(ui, "Pin in gaanim.toml", true)
                    .on_hover_text(
                        "Writes language in the project's gaanim.toml: the talk is dictated \
                         in this language on any computer",
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
                    "Pinned in gaanim.toml as \"{tag}\"; Scribe dictates it as {locale}"
                )));
            }
            (Some(_), Some(_)) => {
                ui.label(faint("Pinned in gaanim.toml (language)".to_owned()));
            }
            (Some(tag), None) => {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(format!(
                            "gaanim.toml asks for \"{tag}\", which Scribe does not transcribe: \
                             {current} is used"
                        ))
                        .size(13.5)
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
                        "gaanim.toml has {} that looks like a credential. Gaanim does not read \
                         it: remove it from the project and, if it reached a repository, \
                         generate a new key.",
                        project
                            .secret_keys
                            .iter()
                            .map(|key| format!("`{key}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .size(13.5)
                    .color(palette::DANGER),
                )
                .wrap(),
            );
        }
        ui.horizontal(|ui| {
            section_label(ui, "Max minutes per session");
            ui.add(
                egui::DragValue::new(&mut settings.scribe_max_minutes)
                    .range(1..=scribe::MAX_SESSION_MINUTES)
                    .suffix(" min"),
            );
        });
        crate::ui_kit::checkbox(
            ui,
            &mut settings.scribe_vocabulary,
            "Send the names of slides and stops as vocabulary",
        );
        ui.add(
            egui::Label::new(faint(format!(
                "While you dictate, your voice is sent to Zoom{}. The token never goes to the \
                 project folder or appears in messages: pasted, it lives only in this session, \
                 and saved, Windows encrypts it. The session ends by itself when it reaches the \
                 minute limit or after two minutes without speech, and never reconnects by \
                 itself.",
                if settings.scribe_vocabulary {
                    ", with the names of the slides"
                } else {
                    ""
                }
            )))
            .wrap(),
        );
    }
    (*settings != before, action)
}

fn path_field(ui: &mut egui::Ui, value: &mut String, title: &str, extensions: &[&str]) {
    // The button first, from the right, so the field takes exactly what is
    // left and the row never grows past its room.
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 0.0),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if small(ui, "…", true).on_hover_text("Browse").clicked()
                && let Some(path) =
                    crate::platform::pick_file(title, None, &[(title, extensions), ("All", &["*"])])
            {
                *value = path.to_string_lossy().into_owned();
            }
            ui.add(
                egui::TextEdit::singleline(value)
                    .desired_width(ui.available_width().max(80.0))
                    .font(egui::FontId::monospace(11.5)),
            );
        },
    );
}

/// The editor's caption, the prompter and the recorder's display in one:
/// the script at the playhead while the speaker opens it (`Shift+T`), and
/// while recording, counting down or finishing a dictation, the script to
/// say, or what Scribe writes of it, over the recording's clock, level and
/// timing.
#[allow(clippy::too_many_arguments)]
pub(crate) fn narration_overlay_system(
    mut contexts: bevy_egui::EguiContexts,
    session: Res<NarrationSession>,
    timeline: Res<Timeline>,
    mut settings: ResMut<PrompterSettings>,
    pace: Res<PrompterPace>,
    editor: Res<EditorState>,
    presentation: Res<PresentationMode>,
    hub: Option<Res<crate::project_hub::ProjectHubState>>,
) {
    let capturing = session.capturing();
    if presentation.active
        || hub.is_some_and(|hub| hub.active)
        || !(capturing || settings.editor_open)
    {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if capturing {
        ctx.request_repaint();
    }
    let screen = ctx.viewport_rect();
    // Above the playback bar while it shows.
    let gap = editor
        .bar_rect
        .map_or(28.0, |bar| (screen.max.y - bar.min.y + 10.0).max(28.0));
    let size = settings.size;
    let id = egui::Id::new("editor-prompter");
    let script = prompter::script_caption(ctx, id, &timeline, &pace);
    let pace_note = script.as_ref().map(|caption| caption.pace.describe());
    let now = Instant::now();
    let warning = match &session.phase {
        Phase::Countdown { input, .. } => input.warning(now),
        Phase::Recording(recording) => recording.input.warning(now),
        _ => None,
    };
    let outline = warning.as_ref().map(LevelWarning::color);
    let contents = |ui: &mut egui::Ui| match &session.phase {
        Phase::Idle => match &script {
            Some(caption) => {
                prompter::caption_text(ui, caption.id, &caption.text, size, 2);
                status_row(ui, Some(&caption.cue), |ui| {
                    slide_label(ui, &caption.part.slide_name, &timeline, &pace);
                });
            }
            None => hint(
                ui,
                id,
                size,
                "Move the playhead onto a slide to read its script",
            ),
        },
        Phase::AwaitingLiveReload { .. } => {
            hint(
                ui,
                id,
                size,
                "Preparing the live take: the scene reloads with its original stops",
            );
            notes_row(ui, "Esc cancels", palette::TEXT_FAINT);
        }
        Phase::Countdown {
            target,
            until,
            input,
            plan,
        } => {
            let remaining = until.saturating_duration_since(now).as_secs_f64();
            target_text(ui, id, size, target, None, script.as_ref());
            status_row(ui, None, |ui| {
                ui.label(
                    egui::RichText::new(format!("{}", remaining.ceil().max(1.0) as u64))
                        .size(24.0)
                        .strong()
                        .color(palette::DANGER),
                );
                let starting = match (target, plan.scribe.is_some()) {
                    (Target::Talk { .. }, true) => {
                        "Timing the talk and dictating its script in a moment".to_owned()
                    }
                    (Target::Talk { .. }, false) => "Timing the talk in a moment".to_owned(),
                    (_, true) => {
                        format!("Recording \"{}\" and dictating in a moment", target.key())
                    }
                    (_, false) => format!("Recording \"{}\" in a moment", target.key()),
                };
                ui.label(
                    egui::RichText::new(starting)
                        .size(15.0)
                        .color(palette::TEXT),
                );
                level_meter(ui, &input.level, 90.0);
            });
            match &warning {
                Some(warning) => notes_row(ui, &warning.message(), warning.color()),
                None => notes_row(
                    ui,
                    &format!(
                        "Microphone: {} · speak as in the take to check your level · Esc cancels",
                        input.recorder.device()
                    ),
                    palette::TEXT_FAINT,
                ),
            }
        }
        Phase::Recording(recording) => {
            let elapsed = recording.input.recorder.elapsed();
            target_text(
                ui,
                id,
                size,
                &recording.target,
                Some(recording),
                script.as_ref(),
            );
            let cue = script
                .as_ref()
                .filter(|_| recording.target.presents())
                .map(|caption| &caption.cue);
            status_row(ui, cue, |ui| {
                recording_dot(ui, &recording.target, recording.dictation.is_some());
                ui.label(
                    egui::RichText::new(format_seconds(elapsed))
                        .monospace()
                        .size(15.0)
                        .color(palette::TEXT_MUTED),
                );
                level_meter(ui, &recording.input.level, 90.0);
                if let Some((slide, spent)) = recording.on_slide(elapsed) {
                    timing_label(
                        ui,
                        slide,
                        spent,
                        &timeline,
                        &pace,
                        &recording.track,
                        elapsed,
                    );
                }
            });
            let (note, color) = match (&warning, &recording.dictation) {
                (Some(warning), _) => (warning.message(), warning.color()),
                (None, Some(dictation)) if dictation.note.is_some() || !dictation.ready => {
                    dictation.state()
                }
                _ => recording_hints(recording, &timeline),
            };
            notes_row(ui, &note, color);
        }
        Phase::Finishing { dictation, .. } => {
            let last = dictation
                .turns
                .iter()
                .rev()
                .take(2)
                .rev()
                .map(|turn| turn.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            prompter::caption_text(
                ui,
                id.with("finishing"),
                &CaptionText::Live {
                    text: last,
                    listening: true,
                },
                size,
                2,
            );
            notes_row(
                ui,
                &format!(
                    "Finishing the dictation… {} phrase(s) received · Esc stops waiting",
                    dictation.turns.len()
                ),
                palette::TEXT_MUTED,
            );
        }
    };
    prompter::show_band(ctx, gap, outline, contents, |ui| {
        caption_menu(ui, &mut settings, pace_note.as_deref(), !capturing);
    });
}

/// The caption's text while recording `target`: a voiceover's text with
/// its markers, else the script at the playhead, or what Scribe has written
/// of a part that has no script yet.
fn target_text(
    ui: &mut egui::Ui,
    id: egui::Id,
    size: f32,
    target: &Target,
    recording: Option<&Recording>,
    script: Option<&prompter::ScriptCaption>,
) {
    match target {
        Target::Voiceover { text, markers, .. } => {
            let tapped = recording.map_or(0, |recording| recording.taps.len());
            let next = markers.get(tapped).map(String::as_str);
            let text = match text {
                Some(text) => {
                    CaptionText::Job(script_job(text, markers, next, ui.available_width()))
                }
                None => CaptionText::Hint(
                    "No text: write it in the script under \"## key\" or pass text=\"…\""
                        .to_owned(),
                ),
            };
            prompter::caption_text(ui, id.with("voiceover"), &text, size, 4);
        }
        Target::Live { .. } | Target::Talk { .. } => {
            let Some(caption) = script else {
                hint(ui, id, size, "The talk goes on outside the slides");
                return;
            };
            let slide = &caption.part.slide_name;
            let dictation = recording
                .and_then(|recording| Some((recording.dictation.as_ref()?, &recording.track[..])));
            let text = match (&caption.text, dictation) {
                (CaptionText::Hint(_), Some((dictation, track))) => {
                    let text = dictation.transcript(track, slide, caption.position.part);
                    if text.is_empty() && !dictation.speaking {
                        CaptionText::Hint(format!(
                            "Speak: Scribe writes this part of {slide} as you go"
                        ))
                    } else {
                        CaptionText::Live {
                            text,
                            listening: dictation.speaking,
                        }
                    }
                }
                (CaptionText::Hint(_), None) => {
                    CaptionText::Hint(format!("No script for this part of {slide}"))
                }
                (text, _) => text.clone(),
            };
            prompter::caption_text(ui, caption.id, &text, size, 2);
        }
    }
}

/// The caption's status line: `left` on the left, the cue to advance on
/// the right.
fn status_row(ui: &mut egui::Ui, cue: Option<&Cue>, left: impl FnOnce(&mut egui::Ui)) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        left(ui);
        if let Some(cue) = cue {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                prompter::cue_label(ui, cue, 15.0);
            });
        }
    });
}

/// A small line under the caption: keys, a warning or Scribe's state.
fn notes_row(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(13.5).color(color)).wrap());
}

/// A caption that only says `text`.
fn hint(ui: &mut egui::Ui, id: egui::Id, size: f32, text: &str) {
    prompter::caption_text(
        ui,
        id.with(text),
        &CaptionText::Hint(text.to_owned()),
        size,
        2,
    );
}

/// The caption's menu: its text size, the pace it reads at, and closing it
/// when no recording owns it.
fn caption_menu(
    ui: &mut egui::Ui,
    settings: &mut PrompterSettings,
    pace: Option<&str>,
    closable: bool,
) {
    if ui
        .add_enabled(settings.can_grow(), egui::Button::new("Larger text"))
        .clicked()
    {
        settings.adjust_size(2.0);
    }
    if ui
        .add_enabled(settings.can_shrink(), egui::Button::new("Smaller text"))
        .clicked()
    {
        settings.adjust_size(-2.0);
    }
    if let Some(pace) = pace {
        ui.separator();
        ui.label(
            egui::RichText::new(pace)
                .size(14.0)
                .color(palette::TEXT_MUTED),
        );
    }
    if closable {
        ui.separator();
        if ui.button("Close · Shift+T").clicked() {
            settings.editor_open = false;
            ui.close();
        }
    }
}

/// The slide's name, with the time it took when last recorded against its
/// target.
fn slide_label(ui: &mut egui::Ui, slide: &str, timeline: &Timeline, pace: &PrompterPace) {
    ui.label(
        egui::RichText::new(slide.to_uppercase())
            .size(14.0)
            .strong()
            .color(palette::TEXT_FAINT),
    );
    let target = timeline
        .segments
        .iter()
        .find(|segment| segment.name == slide)
        .and_then(|segment| segment.target);
    match (pace.timing.slide(slide), target) {
        (Some(recorded), target) => {
            let mut text = pace::format_span(recorded.seconds, false);
            if let Some(target) = target {
                text.push_str(&format!(" of {}", pace::format_span(target, false)));
            }
            ui.label(
                egui::RichText::new(text)
                    .size(14.0)
                    .monospace()
                    .color(timing_color(recorded.seconds, target)),
            )
            .on_hover_text("The time this slide took when you last recorded it");
        }
        (None, Some(target)) => {
            ui.label(
                egui::RichText::new(format!("target {}", pace::format_span(target, false)))
                    .size(14.0)
                    .color(palette::TEXT_FAINT),
            );
        }
        (None, None) => {}
    }
}

/// A blinking dot and what is being recorded.
fn recording_dot(ui: &mut egui::Ui, target: &Target, dictating: bool) {
    let (dot, _) = ui.allocate_exact_size(egui::Vec2::splat(10.0), egui::Sense::hover());
    let blink = (ui.input(|input| input.time) * 2.0) as i64 % 2 == 0;
    let color = if blink {
        palette::DANGER
    } else {
        palette::DANGER.gamma_multiply(0.35)
    };
    ui.painter().circle_filled(dot.center(), 5.0, color);
    let title = match (target, dictating) {
        (Target::Talk { .. }, true) => "DICTATING",
        (Target::Talk { .. }, false) => "TIMING",
        _ => "REC",
    };
    ui.label(
        egui::RichText::new(title)
            .size(14.0)
            .strong()
            .color(palette::TEXT),
    );
}

/// The time on the slide being said against its target, and the talk's
/// expected length against its plan once it can tell.
fn timing_label(
    ui: &mut egui::Ui,
    slide: &str,
    spent: f64,
    timeline: &Timeline,
    pace: &PrompterPace,
    track: &pace::ScriptTrack,
    elapsed: f64,
) {
    let target = timeline
        .segments
        .iter()
        .find(|segment| segment.name == slide)
        .and_then(|segment| segment.target);
    let mut text = format!("{slide} {}", pace::format_span(spent, false));
    if let Some(target) = target {
        text.push_str(&format!(" of {}", pace::format_span(target, false)));
    }
    ui.label(
        egui::RichText::new(text)
            .size(14.5)
            .color(timing_color(spent, target)),
    );
    if let Some((expected, planned)) = expected_talk(timeline, &pace.timing, track, slide, elapsed)
    {
        ui.label(
            egui::RichText::new(format!(
                "talk ≈ {} of {}",
                pace::format_span(expected, false),
                pace::format_span(planned, false)
            ))
            .size(14.5)
            .color(timing_color(expected, Some(planned))),
        )
        .on_hover_text(
            "What the talk would last: this recording so far, plus the times recorded \
             for the slides it does not cover",
        );
    }
}

/// The talk's expected length while recording it, `elapsed` seconds in and
/// on the slide named `current`: the recording so far plus the times
/// recorded for the slides before the first one it covers and after the
/// current one; with its planned length. `None` when one of those slides
/// has no time yet or nothing plans the talk.
fn expected_talk(
    timeline: &Timeline,
    timing: &pace::TalkTiming,
    track: &pace::ScriptTrack,
    current: &str,
    elapsed: f64,
) -> Option<(f64, f64)> {
    let planned = planned_length(timeline)?;
    let slides: Vec<_> = timeline
        .segments
        .iter()
        .filter(|segment| segment.name != IMPLICIT_SEGMENT)
        .collect();
    let index = |name: &str| slides.iter().position(|slide| slide.name == name);
    let first = track
        .iter()
        .find_map(|(_, place)| place.as_ref().map(|(name, _)| name.as_str()))?;
    let (first, current) = (index(first)?, index(current)?);
    let mut expected = elapsed;
    for (position, slide) in slides.iter().enumerate() {
        if position < first || position > current {
            expected += timing.slide(&slide.name)?.seconds;
        }
    }
    Some((expected, planned))
}

/// The keys a recording takes, or a voiceover's next marker, and its color.
fn recording_hints(recording: &Recording, timeline: &Timeline) -> (String, egui::Color32) {
    match &recording.target {
        Target::Voiceover { markers, .. } => {
            let tapped = recording.taps.len();
            match recording.next_marker() {
                Some(marker) => (
                    format!(
                        "Next marker: \"{marker}\" · press Space as you say it ({tapped}/{}) · \
                         Enter: finish · Esc: discard",
                        markers.len()
                    ),
                    palette::ACCENT,
                ),
                None if markers.is_empty() => (
                    "Read the text · Enter: finish · Esc: discard".to_owned(),
                    palette::TEXT_FAINT,
                ),
                None => (
                    "All markers done · Enter: finish · Esc: discard".to_owned(),
                    palette::TEXT_FAINT,
                ),
            }
        }
        Target::Live { .. } | Target::Talk { .. } => {
            let keys = if timeline.current_time >= timeline.cached_duration - 1e-4 {
                "End of the scene · Enter: finish".to_owned()
            } else if let Target::Talk {
                only: Some(scope), ..
            } = &recording.target
            {
                let what = match scope.part {
                    Some(part) => timeline
                        .segments
                        .iter()
                        .find(|segment| segment.name == scope.slide)
                        .map_or_else(
                            || scope.slide.clone(),
                            |segment| format!("{} · {}", scope.slide, part_label(segment, part)),
                        ),
                    None => scope.slide.clone(),
                };
                format!(
                    "Recording {what} again: it ends when you advance out of it · → or Space: \
                     continue · Esc: discard"
                )
            } else {
                "→ or Space: continue · Enter: finish · Esc: discard".to_owned()
            };
            (keys, palette::TEXT_FAINT)
        }
    }
}

/// Input meter in dBFS with the ideal zone in green, the edge of clipping
/// in amber and red, and the current peak.
fn level_meter(ui: &mut egui::Ui, level: &InputLevel, width: f32) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
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
        "Ideal peaks between {:.0} and {:.0} dBFS (green). Above {:.0} dBFS a \
         loud word may clip, and clipping cannot be fixed afterwards.",
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
            .size(14.0)
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
        assert_eq!(loudness_detail(None), " · not leveled");
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
    fn the_script_opens_at_what_completing_added() {
        let slides = vec![
            slide("Portada", None, &[]),
            slide("Datos", None, &[Some("grafico")]),
        ];
        let manifest = NarrationManifest::default();
        let line_of =
            |source: &str, text: &str| source.lines().position(|line| line == text).unwrap() + 1;
        // A new script opens at its first section.
        let (created, _) = complete_source(None, &manifest, &slides).unwrap();
        assert_eq!(script_line(None, &created), line_of(&created, "## Portada"));

        // A section added between two others opens at its heading.
        let before = "# Script\n\n## Portada\n\nHola.\n\n## Datos\n\nMiren.\n\n[stop grafico]\n";
        let slides = vec![
            slide("Portada", None, &[]),
            slide("Método", None, &[]),
            slide("Datos", None, &[Some("grafico")]),
        ];
        let (completed, _) = complete_source(Some(before), &manifest, &slides).unwrap();
        assert_eq!(
            script_line(Some(before), &completed),
            line_of(&completed, "## Método")
        );

        // A mark added at the end opens at the mark.
        let slides = vec![
            slide("Portada", None, &[]),
            slide("Datos", None, &[Some("grafico"), Some("fin")]),
        ];
        let (completed, _) = complete_source(Some(before), &manifest, &slides).unwrap();
        assert_eq!(
            script_line(Some(before), &completed),
            line_of(&completed, "[stop fin]")
        );

        // Nothing to add: the first section without text, else the first.
        let empty = "# Script\n\n## Portada\n\nHola.\n\n## Datos\n\n[stop grafico]\n";
        assert_eq!(script_line(Some(empty), empty), line_of(empty, "## Datos"));
        assert_eq!(
            script_line(Some(before), before),
            line_of(before, "## Portada")
        );
    }

    #[test]
    fn dictation_fills_empty_parts_and_replaces_written_ones() {
        let slides = vec![
            slide("Portada", None, &[Some("titulo")]),
            slide("Datos", None, &[Some("grafico"), None]),
        ];
        let existing = "# Script\n\n## Datos\n\nYa escrito.\n\n[stop grafico]\n\n[stop]\n";
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
                replaced: 1,
                commented: 0
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
                    "Miren --> esto.\n\n[stop grafico]\n\nCrece.\n\n[stop]\n\nGracias.".to_owned()
                ),
            ]
        );
        // The text it replaced stays below it, as a comment.
        assert!(
            source.contains("Miren --> esto.\n<!-- previous: Ya escrito. -->"),
            "{source}"
        );
        assert!(source.contains("[stop titulo]\n\n## Datos"), "{source}");
    }

    #[test]
    fn recording_again_keeps_one_previous_text_and_the_speakers_comments() {
        let slides = vec![slide("Datos", None, &[Some("grafico")])];
        let existing = "## Datos\n\nPrimer dictado.\n<!-- una nota -->\n<!-- dictated: viejo -->\n\
                        <!-- previous: más viejo -->\n\n[stop grafico]\n\nEscrito a mano.\n";
        let dictated = vec![
            ("Datos".to_owned(), 0, "Segundo dictado.".to_owned()),
            ("Datos".to_owned(), 1, "Escrito  a\nmano.".to_owned()),
        ];
        let (source, report) = merge_dictation(Some(existing), &slides, &dictated).unwrap();
        assert_eq!(
            report,
            DictationReport {
                parts: 2,
                replaced: 2,
                commented: 0
            }
        );
        // Gaanim's earlier comments give way to one with the text replaced;
        // the speaker's own stays. Saying the same again leaves no comment.
        assert_eq!(
            source,
            "## Datos\n\nSegundo dictado.\n<!-- previous: Primer dictado. -->\n<!-- una nota -->\n\n\
             [stop grafico]\n\nEscrito  a\nmano.\n"
        );
        // Text and a comment on one line: never replaced.
        let mixed = "## Datos\n\nPrimer dictado. <!-- nota -->\n\n[stop grafico]\n";
        let (source, report) = merge_dictation(Some(mixed), &slides, &dictated[..1]).unwrap();
        assert_eq!((report.replaced, report.commented), (0, 1));
        assert!(
            source.contains("<!-- dictated: Segundo dictado. -->\n\n[stop grafico]"),
            "{source}"
        );
    }

    #[test]
    fn a_recording_of_the_talk_starts_at_a_slide_or_a_stop() {
        use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};
        let slide = |id, name: &str, start, end, stops: &[f64]| SegmentMetadata {
            id,
            name: name.into(),
            notes: None,
            target: None,
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
        };
        let mut timeline = Timeline::new();
        timeline.cached_duration = 6.0;
        timeline.set_segments(vec![
            slide(1, IMPLICIT_SEGMENT, 0.0, 0.0, &[]),
            slide(2, "Portada", 0.0, 2.0, &[1.0]),
            slide(3, "Datos", 2.0, 6.0, &[3.0, 4.0]),
            slide(4, "Cierre", 6.0, 7.0, &[7.0]),
        ]);
        // The first slide, past the segment before the first one.
        assert_eq!(talk_start(&timeline, None, None), Some(0.0));
        assert_eq!(talk_start(&timeline, Some("Datos"), None), Some(2.0));
        assert_eq!(talk_start(&timeline, Some("Otra"), None), None);
        // A part starts at the stop before it; the first, with its slide.
        assert_eq!(talk_start(&timeline, Some("Datos"), Some(0)), Some(2.0));
        assert_eq!(talk_start(&timeline, Some("Datos"), Some(2)), Some(4.0));
        assert_eq!(talk_start(&timeline, Some("Datos"), Some(5)), None);
        // Datos goes on past its last stop; Cierre ends at its own.
        let datos = &timeline.segments[2];
        assert_eq!(slide_parts(datos), 3);
        assert_eq!(slide_parts(&timeline.segments[3]), 1);
        assert_eq!(part_label(datos, 1), "Step 2");
        assert_eq!(part_label(datos, 2), "until the next slide");

        // Recording one slide or part again ends once the speaker leaves it.
        let place = |time: f64, slide: &str, part| (time, Some((slide.to_owned(), part)));
        let slide_scope = Scope {
            slide: "Datos".into(),
            part: None,
        };
        let part_scope = |part| Scope {
            slide: "Datos".into(),
            part: Some(part),
        };
        let within = [place(0.0, "Datos", 0), place(2.0, "Datos", 1)];
        assert!(!left_scope(&within, &slide_scope));
        assert!(left_scope(&within, &part_scope(0)));
        assert!(!left_scope(&within, &part_scope(1)));
        assert!(left_scope(
            &[place(0.0, "Datos", 1), place(3.0, "Cierre", 0)],
            &slide_scope
        ));
        assert!(left_scope(
            &[place(0.0, "Datos", 0), (3.0, None)],
            &slide_scope
        ));
        assert!(!left_scope(&[place(0.0, "Cierre", 0)], &slide_scope));
    }

    #[test]
    fn the_timing_says_how_the_talk_fits_its_plan() {
        assert_eq!(timing_color(30.0, Some(30.0)), palette::LOOP);
        assert_eq!(timing_color(38.0, Some(30.0)), palette::STOP);
        assert_eq!(timing_color(45.0, Some(30.0)), palette::DANGER);
        // A tenth of a long target.
        assert_eq!(timing_color(215.0, Some(200.0)), palette::STOP);
        assert_eq!(timing_color(45.0, None), palette::TEXT_MUTED);
        assert_eq!(timing_verdict(100.0, 0, None), None);
        let verdict = |total, missing| timing_verdict(total, missing, Some(240.0)).unwrap();
        assert_eq!(
            verdict(232.0, 0),
            ("Fits the plan with 0:08 to spare".to_owned(), palette::LOOP)
        );
        assert_eq!(verdict(252.0, 0).1, palette::STOP);
        assert_eq!(verdict(300.0, 0).1, palette::DANGER);
        assert_eq!(
            verdict(118.0, 2).0,
            "2:02 left for the 2 slide(s) without a time"
        );
        assert_eq!(verdict(250.0, 1).1, palette::DANGER);
    }

    #[test]
    fn the_talks_length_is_expected_from_the_recording_and_the_times_kept() {
        use gaanim_timeline::timeline::SegmentMetadata;
        let slide = |id, name: &str, target| SegmentMetadata {
            id,
            name: name.into(),
            notes: None,
            target,
            start_time: f64::from(id),
            end_time: f64::from(id) + 1.0,
            stops: Vec::new(),
        };
        let mut timeline = Timeline::new();
        timeline.set_segments(vec![
            slide(0, "Portada", Some(30.0)),
            slide(1, "Datos", Some(60.0)),
            slide(2, "Cierre", Some(20.0)),
        ]);
        assert_eq!(planned_length(&timeline), Some(110.0));
        let mut timing = pace::TalkTiming::default();
        let kept = |name: &str, seconds| SlideTiming {
            name: name.into(),
            seconds,
            parts: vec![seconds],
            recorded: 0,
        };
        timing.record(kept("Portada", 25.0));
        let track = [(0.0, Some(("Datos".to_owned(), 0)))];
        // Cierre has no time yet: nothing to expect.
        assert_eq!(
            expected_talk(&timeline, &timing, &track, "Datos", 40.0),
            None
        );
        timing.record(kept("Cierre", 18.0));
        assert_eq!(
            expected_talk(&timeline, &timing, &track, "Datos", 40.0),
            Some((83.0, 110.0))
        );
        timeline.talk_target = Some(240.0);
        assert_eq!(planned_length(&timeline), Some(240.0));
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
        // Settings given here: loading them would read the user's own file.
        let mut panel = NarrationPanel {
            scribe_token: "secret-token".to_owned(),
            settings: Some(NarrationSettings::default()),
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
