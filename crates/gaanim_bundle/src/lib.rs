//! Playback bundles: a scene or presentation recorded into one file that
//! replays without Python.
//!
//! A bundle records, at the fixed frame grid of an export, what the 2D
//! renderer draws after the timeline, updaters, reactive bindings and every
//! Python callback have run: each drawable as a [`FragmentRecipe`] and how
//! the frame composites it. Replaying builds the fragments with the same
//! [`gaanim_renderer::fragment::build_fragment`] and composites them with the
//! same code, so a bundle frame draws exactly what the scene drew.
//!
//! The file is a ZIP archive:
//!
//! - `manifest.json`: format version, summary and the BLAKE3 hash of every
//!   other entry.
//! - `scene.bin`: static data (background, post-process shaders, timeline
//!   structure, audio tracks).
//! - `tables/*.bin`: deduplicated paths, images, recipes, strings, shader
//!   transitions and the passes of shader effects on drawables.
//! - `frames/NNNNNN.bin`: chunks of frames, each frame encoded against the
//!   one before it; a chunk starts with a whole frame.
//! - `media/*`: embedded audio files.
//! - `polls.json`: audience polls, their relay session and the elements
//!   drawn from live poll data (bars, nicknames, readouts) with the glyphs
//!   to redraw them, only when the scene has polls. Readers that predate
//!   it ignore it, so it needs no new format version.
//!
//! A bundle opens from its bytes whole or in pieces ([`BundleSource`]): the
//! web player opens one from the end of the file, which holds the archive's
//! directory and every table, and downloads frame chunks as playback needs
//! them.
//!
//! [`FragmentRecipe`]: gaanim_renderer::fragment::FragmentRecipe

mod archive;
mod codec;
mod digest;
mod model;
mod source;

use std::collections::HashMap;
use std::io::{Read, Seek, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gaanim_renderer::background::{BackgroundPaint, ShaderBackground, ShaderBackgroundRequest};
use gaanim_renderer::fragment::FragmentStore;
use gaanim_renderer::pipeline::{
    CanvasBackground, CapturedElement, ComposedFrame, SegmentBackgroundPaint,
    compose_captured_frame,
};
use gaanim_renderer::post_process::PostProcessShader;
use gaanim_timeline::timeline::{
    GateCondition, PollSessionInfo, SegmentMetadata, SegmentStop, StopGate, TimelineMarker,
    TimelinePoll, TimelineQuiz,
};
use serde::{Deserialize, Serialize};

use archive::Archive;
use codec::{Reader, Writer};
pub use digest::scene_digest;
use model::{DecodedTables, DeltaDecoder, DeltaEncoder, FrameRecord, Tables};
pub use model::{EntityKeys, Frame, PostPass};
pub use source::BundleSource;

/// The entity that stands for element key `key` in decoded frames.
pub fn element_entity(key: u32) -> Result<bevy::prelude::Entity> {
    model::key_entity(key)
}

/// Identifies the file type in `manifest.json`.
pub const FORMAT: &str = "gaanim-bundle";
/// Version of the bundle encoding this build writes and reads; bundles of
/// any other version are refused.
///
/// Every data entry holds a Zstandard frame; `manifest.json` stays Deflate
/// and media stays as authored. Version 1 (Gaanim 0.6.0) left compression
/// to the archive; version 2 (up to Gaanim 0.8) had no shader transitions
/// (`tables/transitions.bin`) nor the opacity of each element's group;
/// version 3 (development builds of Gaanim 0.9) had no chalk strokes nor
/// shader effects on drawables (`tables/effects.bin`), track mattes nor
/// glass; version 4 (development builds of Gaanim 0.9) had no reactive
/// uniforms on shader transitions nor shader backgrounds.
pub const VERSION: u32 = 5;
/// Zstandard level of data entries, still fast to read. Level 17 made
/// entries about 8% smaller and compressed nearly four times slower.
#[cfg(not(target_arch = "wasm32"))]
const ZSTD_LEVEL: i32 = 15;
/// File extension of playback bundles.
pub const EXTENSION: &str = "gaanim";
/// Optional cover image: a PNG stored as-is, which file managers show
/// without decoding the bundle (the `gaanim_thumbnail` crate reads it).
pub const THUMBNAIL: &str = "thumbnail.png";
/// Optional audience polls, as JSON.
const POLLS: &str = "polls.json";
/// Live zones, as JSON: the player runs them, so frames never hold them.
const LIVE: &str = "live.json";
/// The scene's rehearsal, as JSON: live zones replay it outside a
/// presentation.
const REHEARSAL: &str = "rehearsal.json";
/// Frames per chunk: one second at the default rate.
const CHUNK_FRAMES: usize = 60;

#[derive(Debug, thiserror::Error)]
pub enum BundleError {
    #[error("could not read or write the bundle: {0}")]
    Io(#[from] std::io::Error),
    #[error("the bundle archive is damaged: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("the bundle is damaged: {0}")]
    Corrupt(String),
    #[error("{}", version_message(*found, generator))]
    UnsupportedVersion { found: u32, generator: String },
    #[error("{0}")]
    Unsupported(String),
    /// Reading needs bytes of the file that have not arrived yet.
    #[error("the bundle is still downloading")]
    Incomplete { missing: Vec<Range<u64>> },
}

type Result<T> = std::result::Result<T, BundleError>;

fn version_message(found: u32, generator: &str) -> String {
    if found < VERSION {
        format!(
            "this bundle was recorded by {generator} in format version {found}, which this \
             Gaanim no longer reads (it reads version {VERSION}). Record it again from its \
             script with `gaanim export <script> --output <file>.gaanim`"
        )
    } else {
        format!(
            "this bundle uses format version {found}; this Gaanim reads version {VERSION}. \
             Open it with the Gaanim that wrote it ({generator}) or a newer one"
        )
    }
}

// ---------------------------------------------------------------------------
// Static scene data
// ---------------------------------------------------------------------------

/// One authored scene of the timeline, for chapter navigation.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSpan {
    pub name: String,
    pub start: f64,
    pub end: f64,
}

/// An audio track, its file embedded under `media/`.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioData {
    /// Entry name of the embedded file.
    pub media: String,
    pub start_time: f64,
    pub duration: Option<f64>,
    pub volume: f64,
    pub fade_in: f64,
    pub fade_out: f64,
    pub source_offset: f64,
    pub source_duration: Option<f64>,
    pub speed: f64,
    pub looping: bool,
}

/// Everything in a bundle that does not change from frame to frame.
#[derive(Clone, Default)]
pub struct SceneData {
    /// Name shown for the bundle, usually the script's.
    pub title: String,
    /// Frame grid of the recording.
    pub fps: u32,
    /// Timeline length in seconds.
    pub duration: f64,
    /// Authored output size in pixels.
    pub output_size: (u32, u32),
    /// Color outside the frame, as an 8-bit sRGB RGBA value.
    pub clear_color: Option<[u8; 4]>,
    pub background: Option<CanvasBackground>,
    /// Shaders that post-process passes reference by index.
    pub post_shaders: Vec<PostProcessShader>,
    pub segments: Vec<SegmentMetadata>,
    pub markers: Vec<TimelineMarker>,
    pub scenes: Vec<SceneSpan>,
    pub audio: Vec<AudioData>,
    /// Audience polls, stored in their own entry (see [`POLLS`]).
    pub polls: Vec<TimelinePoll>,
    pub poll_session: Option<PollSessionInfo>,
    /// Stops that advance once the audience meets a condition.
    pub stop_gates: Vec<StopGate>,
    /// Live zones, which the player runs while presenting.
    pub live_zones: Vec<gaanim_animation::live::LiveZone>,
    /// The made-up audience live zones replay outside a presentation.
    pub rehearsal: Option<gaanim_animation::rehearsal::Rehearsal>,
    /// Elements drawn as poll bars, which a live presentation redraws.
    pub poll_bars: Vec<PollBarRecord>,
    /// Elements drawn as live text, such as leaderboard nicknames.
    pub poll_texts: Vec<LiveTextRecord>,
    /// Readouts of live poll values.
    pub poll_readouts: Vec<LiveReadoutRecord>,
}

// A presented bundle redraws these recorded elements from the live results:
// it replaces an element's outline and keeps everything else the frame
// recorded (paint, transform, opacity, effects).

/// One character of a glyph atlas: its outline (SVG path data) and advance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlyphRecord {
    pub ch: char,
    pub advance: f64,
    pub path: String,
}

/// A run of text shaped as a whole, such as a readout's prefix.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RunRecord {
    pub advance: f64,
    pub path: String,
}

/// Where a live value comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveSourceRecord {
    /// `measure` is `votes`, `share`, `percent`, `total` or `remaining`.
    Poll {
        poll: String,
        answers: usize,
        measure: String,
        #[serde(default)]
        answer: usize,
        #[serde(default)]
        time: f64,
    },
    LeaderScore {
        rank: usize,
    },
    Players,
    AudienceJoined {
        slot: usize,
    },
    AudienceAge {
        slot: usize,
    },
    /// `measure` is `score`, `players` or `average`.
    Team {
        team: usize,
        measure: String,
    },
    LeadingTeam,
    /// A number about the player at `index` of `list` (`audience` or
    /// `leaderboard`); `measure` is `score`, `correct`, `answered`,
    /// `streak`, or `responded`, `chose`, `right`, `points` or `time` on
    /// `poll` (`chose` of `option`).
    Player {
        list: String,
        index: usize,
        measure: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        poll: String,
        #[serde(default)]
        option: usize,
    },
}

/// What a bar's length follows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BarSourceRecord {
    Answer {
        poll: String,
        answer: usize,
        answers: usize,
    },
    Leader {
        rank: usize,
    },
    Team {
        team: usize,
    },
}

/// A recorded element whose outline is a poll bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PollBarRecord {
    /// Element key in the recorded frames.
    pub key: u32,
    pub source: BarSourceRecord,
    pub length: f64,
    pub thickness: f64,
    pub radius: f64,
    /// `right`, `left`, `up` or `down`.
    pub direction: String,
    /// `total` or `leader`.
    pub scale: String,
}

/// A recorded element whose outline is a player's nickname.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveTextRecord {
    pub key: u32,
    /// The list the nickname comes from: empty for the leaderboard, where
    /// `rank` is the place, or `audience`, where it is the joining order.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub list: String,
    pub rank: usize,
    /// `left`, `center` or `right`.
    pub align: String,
    pub glyphs: Vec<GlyphRecord>,
}

/// A recorded readout of a live value: its format and the glyphs to draw
/// any number with it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveReadoutRecord {
    pub key: u32,
    pub source: LiveSourceRecord,
    pub format: String,
    pub invalid: String,
    pub decimal_separator: char,
    pub prefix: RunRecord,
    pub suffix: RunRecord,
    pub glyphs: Vec<GlyphRecord>,
}

/// `polls.json`.
#[derive(Serialize, Deserialize)]
struct PollsEntry {
    #[serde(default)]
    session: Option<SessionRecord>,
    polls: Vec<PollRecord>,
    #[serde(default)]
    bars: Vec<PollBarRecord>,
    #[serde(default)]
    texts: Vec<LiveTextRecord>,
    #[serde(default)]
    readouts: Vec<LiveReadoutRecord>,
    #[serde(default)]
    gates: Vec<GateRecord>,
}

/// A stop that advances by itself, and its condition.
#[derive(Serialize, Deserialize)]
struct GateRecord {
    time: f64,
    until: ConditionRecord,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ConditionRecord {
    Answers {
        poll: String,
        count: u32,
    },
    AnswerShare {
        poll: String,
        share: f64,
        players: bool,
    },
    TimeUp {
        poll: String,
    },
    Players {
        count: u32,
    },
    All {
        of: Vec<ConditionRecord>,
    },
    Any {
        of: Vec<ConditionRecord>,
    },
}

impl ConditionRecord {
    fn of(condition: &GateCondition) -> Self {
        match condition {
            GateCondition::Answers { poll, count } => Self::Answers {
                poll: poll.clone(),
                count: *count,
            },
            GateCondition::AnswerShare {
                poll,
                share,
                players,
            } => Self::AnswerShare {
                poll: poll.clone(),
                share: *share,
                players: *players,
            },
            GateCondition::TimeUp { poll } => Self::TimeUp { poll: poll.clone() },
            GateCondition::Players { count } => Self::Players { count: *count },
            GateCondition::All(conditions) => Self::All {
                of: conditions.iter().map(Self::of).collect(),
            },
            GateCondition::Any(conditions) => Self::Any {
                of: conditions.iter().map(Self::of).collect(),
            },
        }
    }

    fn condition(self) -> GateCondition {
        match self {
            Self::Answers { poll, count } => GateCondition::Answers { poll, count },
            Self::AnswerShare {
                poll,
                share,
                players,
            } => GateCondition::AnswerShare {
                poll,
                share,
                players,
            },
            Self::TimeUp { poll } => GateCondition::TimeUp { poll },
            Self::Players { count } => GateCondition::Players { count },
            Self::All { of } => GateCondition::All(of.into_iter().map(Self::condition).collect()),
            Self::Any { of } => GateCondition::Any(of.into_iter().map(Self::condition).collect()),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct SessionRecord {
    relay: Option<String>,
    code: String,
    #[serde(default)]
    lobby: bool,
    #[serde(default)]
    game_segment: Option<u32>,
    #[serde(default)]
    teams: Option<TeamsRecord>,
    #[serde(default)]
    ask: Option<AskRecord>,
}

#[derive(Serialize, Deserialize)]
struct AskRecord {
    label: String,
    #[serde(default)]
    required: bool,
}

#[derive(Serialize, Deserialize)]
struct TeamsRecord {
    names: Vec<String>,
    colors: Vec<String>,
    #[serde(default)]
    choose: bool,
}

#[derive(Serialize, Deserialize)]
struct PollRecord {
    id: String,
    question: String,
    options: Vec<String>,
    preview: Vec<u32>,
    #[serde(default)]
    segment: u32,
    open: f64,
    close: f64,
    #[serde(default)]
    quiz: Option<QuizRecord>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    multiple: bool,
    /// The picture's hash and type; its bytes are the entry `images/<hash>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    image: Option<ImageRecord>,
}

#[derive(Serialize, Deserialize)]
struct ImageRecord {
    hash: String,
    mime: String,
}

/// A quiz's right answers: one number, as bundles wrote them before
/// multiple choice, or a list.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum CorrectRecord {
    One(usize),
    Many(Vec<usize>),
}

impl CorrectRecord {
    fn of(correct: &[usize]) -> Self {
        match correct {
            [one] => Self::One(*one),
            many => Self::Many(many.to_vec()),
        }
    }

    fn answers(self) -> Vec<usize> {
        match self {
            Self::One(one) => vec![one],
            Self::Many(many) => many,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct QuizRecord {
    correct: CorrectRecord,
    time: u32,
    points: u32,
    reveal: Option<f64>,
}

fn write_polls(scene: &SceneData) -> Result<Vec<u8>> {
    let entry = PollsEntry {
        session: scene.poll_session.as_ref().map(|session| SessionRecord {
            relay: session.relay.clone(),
            code: session.code.clone(),
            lobby: session.lobby,
            game_segment: session.game_segment,
            teams: session.teams.as_ref().map(|teams| TeamsRecord {
                names: teams.names.clone(),
                colors: teams.colors.clone(),
                choose: teams.choose,
            }),
            ask: session.ask.as_ref().map(|ask| AskRecord {
                label: ask.label.clone(),
                required: ask.required,
            }),
        }),
        polls: scene
            .polls
            .iter()
            .map(|poll| PollRecord {
                id: poll.id.clone(),
                question: poll.question.clone(),
                options: poll.options.clone(),
                preview: poll.preview.clone(),
                segment: poll.segment,
                open: poll.open,
                close: poll.close,
                quiz: poll.quiz.as_ref().map(|quiz| QuizRecord {
                    correct: CorrectRecord::of(&quiz.correct),
                    time: quiz.time,
                    points: quiz.points,
                    reveal: quiz.reveal,
                }),
                multiple: poll.multiple,
                image: poll.image.as_ref().map(|image| ImageRecord {
                    hash: image.hash.clone(),
                    mime: image.mime.clone(),
                }),
            })
            .collect(),
        bars: scene.poll_bars.clone(),
        texts: scene.poll_texts.clone(),
        readouts: scene.poll_readouts.clone(),
        gates: scene
            .stop_gates
            .iter()
            .map(|gate| GateRecord {
                time: gate.time,
                until: ConditionRecord::of(&gate.until),
            })
            .collect(),
    };
    serde_json::to_vec(&entry).map_err(|error| BundleError::Corrupt(error.to_string()))
}

/// The entry holding a poll picture's bytes.
fn image_entry(hash: &str) -> String {
    format!("images/{hash}")
}

/// Read the polls entry; `image` reads a picture's bytes by entry name.
fn read_polls(
    bytes: &[u8],
    scene: &mut SceneData,
    mut image: impl FnMut(&str) -> Result<Vec<u8>>,
) -> Result<()> {
    let entry: PollsEntry = serde_json::from_slice(bytes)
        .map_err(|error| BundleError::Corrupt(format!("{POLLS}: {error}")))?;
    scene.poll_session = entry.session.map(|session| PollSessionInfo {
        relay: session.relay,
        code: session.code,
        lobby: session.lobby,
        game_segment: session.game_segment,
        teams: session
            .teams
            .map(|teams| gaanim_timeline::timeline::TeamsInfo {
                names: teams.names,
                colors: teams.colors,
                choose: teams.choose,
            }),
        ask: session.ask.map(|ask| gaanim_timeline::timeline::AskInfo {
            label: ask.label,
            required: ask.required,
        }),
    });
    scene.polls = entry
        .polls
        .into_iter()
        .map(|poll| {
            let image = match poll.image {
                Some(record) => Some(gaanim_timeline::timeline::PollImage {
                    bytes: image(&image_entry(&record.hash))?.into(),
                    hash: record.hash,
                    mime: record.mime,
                }),
                None => None,
            };
            Ok(TimelinePoll {
                id: poll.id,
                question: poll.question,
                options: poll.options,
                preview: poll.preview,
                segment: poll.segment,
                open: poll.open,
                close: poll.close,
                quiz: poll.quiz.map(|quiz| TimelineQuiz {
                    correct: quiz.correct.answers(),
                    time: quiz.time,
                    points: quiz.points,
                    reveal: quiz.reveal,
                }),
                multiple: poll.multiple,
                image,
            })
        })
        .collect::<Result<_>>()?;
    scene.poll_bars = entry.bars;
    scene.poll_texts = entry.texts;
    scene.poll_readouts = entry.readouts;
    scene.stop_gates = entry
        .gates
        .into_iter()
        .map(|gate| StopGate {
            time: gate.time,
            until: gate.until.condition(),
        })
        .collect();
    Ok(())
}

fn write_paint(w: &mut Writer, tables: &mut Tables, paint: &BackgroundPaint) {
    match paint {
        BackgroundPaint::Brush(brush) => {
            w.u8(0);
            codec::write_brush(w, tables, brush);
        }
        BackgroundPaint::Shader(shader) => {
            w.u8(1);
            w.str(shader.source());
            codec::write_color(w, shader.fallback());
            // Frames record the values; playback only needs the names.
            w.len(shader.uniforms().len());
            for name in shader.uniforms() {
                w.str(name);
            }
            w.option(shader.data(), |w, data| {
                w.len(data.len());
                for texel in data.iter() {
                    for value in texel {
                        w.f32(*value);
                    }
                }
            });
        }
    }
}

fn read_paint(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<BackgroundPaint> {
    match r.u8()? {
        0 => Ok(BackgroundPaint::Brush(codec::read_brush(r, tables)?)),
        1 => {
            let source = r.str()?.to_owned();
            let fallback = codec::read_color(r)?;
            let count = r.len()?;
            let mut uniforms = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                uniforms.push((
                    r.str()?.to_owned(),
                    gaanim_animation::ScalarSource::constant(0.0),
                ));
            }
            let data = r.option(|r| {
                let count = r.len()?;
                let mut data = Vec::with_capacity(count.min(1 << 20));
                for _ in 0..count {
                    data.push([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
                }
                Ok(data)
            })?;
            ShaderBackground::with_uniforms(source, fallback, uniforms, data.map(Into::into))
                .map(BackgroundPaint::Shader)
                .map_err(|error| BundleError::Corrupt(error.to_string()))
        }
        _ => Err(BundleError::Corrupt("invalid background paint".into())),
    }
}

fn write_opt_f64(w: &mut Writer, value: Option<f64>) {
    w.option(value, Writer::f64);
}

impl SceneData {
    fn write(&self, w: &mut Writer, tables: &mut Tables) {
        w.str(&self.title);
        w.var(u64::from(self.fps));
        w.f64(self.duration);
        w.var(u64::from(self.output_size.0));
        w.var(u64::from(self.output_size.1));
        w.option(self.clear_color, |w, rgba| {
            for channel in rgba {
                w.u8(channel);
            }
        });
        w.option(self.background.as_ref(), |w, background| {
            write_paint(w, tables, &background.paint);
            w.len(background.segment_paints.len());
            for segment in &background.segment_paints {
                w.f64(segment.start_time);
                w.f64(segment.end_time);
                w.option(segment.paint.as_ref(), |w, paint| {
                    write_paint(w, tables, paint)
                });
                w.bool(segment.hold_at_end);
            }
            w.var(u64::from(background.pixel_size.0));
            w.var(u64::from(background.pixel_size.1));
            let bounds = &background.bounds;
            for value in [
                bounds.min.x,
                bounds.min.y,
                bounds.min.z,
                bounds.max.x,
                bounds.max.y,
                bounds.max.z,
            ] {
                w.f64(value);
            }
        });
        w.len(self.post_shaders.len());
        for shader in &self.post_shaders {
            model::write_post_shader(w, shader);
        }
        w.len(self.segments.len());
        for segment in &self.segments {
            w.var(u64::from(segment.id));
            w.str(&segment.name);
            w.option(segment.notes.as_deref(), Writer::str);
            w.f64(segment.start_time);
            w.f64(segment.end_time);
            w.len(segment.stops.len());
            for stop in &segment.stops {
                w.option(stop.name.as_deref(), Writer::str);
                w.f64(stop.time);
                write_opt_f64(w, stop.ambient);
            }
        }
        w.len(self.markers.len());
        for marker in &self.markers {
            w.str(&marker.name);
            w.f64(marker.time);
        }
        w.len(self.scenes.len());
        for scene in &self.scenes {
            w.str(&scene.name);
            w.f64(scene.start);
            w.f64(scene.end);
        }
        w.len(self.audio.len());
        for audio in &self.audio {
            w.str(&audio.media);
            w.f64(audio.start_time);
            write_opt_f64(w, audio.duration);
            w.f64(audio.volume);
            w.f64(audio.fade_in);
            w.f64(audio.fade_out);
            w.f64(audio.source_offset);
            write_opt_f64(w, audio.source_duration);
            w.f64(audio.speed);
            w.bool(audio.looping);
        }
    }

    fn read(r: &mut Reader<'_>, tables: &DecodedTables) -> Result<Self> {
        let title = r.str()?.to_owned();
        let fps = r.u32()?;
        let duration = r.f64()?;
        let output_size = (r.u32()?, r.u32()?);
        let clear_color = r.option(|r| Ok([r.u8()?, r.u8()?, r.u8()?, r.u8()?]))?;
        let background = r.option(|r| {
            let paint = read_paint(r, tables)?;
            let count = r.len()?;
            let mut segment_paints = Vec::with_capacity(count.min(4096));
            for _ in 0..count {
                segment_paints.push(SegmentBackgroundPaint {
                    start_time: r.f64()?,
                    end_time: r.f64()?,
                    paint: r.option(|r| read_paint(r, tables))?,
                    hold_at_end: r.bool()?,
                });
            }
            let pixel_size = (r.u32()?, r.u32()?);
            let bounds = gaanim_math::Bounds3D::new_3d(
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
                r.f64()?,
            );
            Ok(CanvasBackground {
                paint,
                segment_paints,
                pixel_size,
                bounds,
                parameters: Vec::new(),
            })
        })?;
        let shader_count = r.len()?;
        let mut post_shaders = Vec::with_capacity(shader_count.min(256));
        for _ in 0..shader_count {
            post_shaders.push(model::read_post_shader(r)?);
        }
        let segment_count = r.len()?;
        let mut segments = Vec::with_capacity(segment_count.min(4096));
        for _ in 0..segment_count {
            let id = r.u32()?;
            let name = r.str()?.to_owned();
            let notes = r.option(|r| Ok(r.str()?.to_owned()))?;
            let start_time = r.f64()?;
            let end_time = r.f64()?;
            let stop_count = r.len()?;
            let mut stops = Vec::with_capacity(stop_count.min(4096));
            for _ in 0..stop_count {
                stops.push(SegmentStop {
                    name: r.option(|r| Ok(r.str()?.to_owned()))?,
                    time: r.f64()?,
                    ambient: r.option(Reader::f64)?,
                });
            }
            segments.push(SegmentMetadata {
                id,
                name,
                notes,
                start_time,
                end_time,
                stops,
            });
        }
        let marker_count = r.len()?;
        let mut markers = Vec::with_capacity(marker_count.min(4096));
        for _ in 0..marker_count {
            markers.push(TimelineMarker {
                name: r.str()?.to_owned(),
                time: r.f64()?,
            });
        }
        let scene_count = r.len()?;
        let mut scenes = Vec::with_capacity(scene_count.min(4096));
        for _ in 0..scene_count {
            scenes.push(SceneSpan {
                name: r.str()?.to_owned(),
                start: r.f64()?,
                end: r.f64()?,
            });
        }
        let audio_count = r.len()?;
        let mut audio = Vec::with_capacity(audio_count.min(256));
        for _ in 0..audio_count {
            audio.push(AudioData {
                media: r.str()?.to_owned(),
                start_time: r.f64()?,
                duration: r.option(Reader::f64)?,
                volume: r.f64()?,
                fade_in: r.f64()?,
                fade_out: r.f64()?,
                source_offset: r.f64()?,
                source_duration: r.option(Reader::f64)?,
                speed: r.f64()?,
                looping: r.bool()?,
            });
        }
        Ok(Self {
            title,
            fps,
            duration,
            output_size,
            clear_color,
            background,
            post_shaders,
            segments,
            markers,
            scenes,
            audio,
            polls: Vec::new(),
            poll_session: None,
            stop_gates: Vec::new(),
            live_zones: Vec::new(),
            rehearsal: None,
            poll_bars: Vec::new(),
            poll_texts: Vec::new(),
            poll_readouts: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// Manifest
// ---------------------------------------------------------------------------

/// Human-readable summary and integrity table, stored as `manifest.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub format: String,
    pub version: u32,
    /// Program that wrote the bundle, e.g. `gaanim 0.5.2`.
    pub generator: String,
    pub title: String,
    pub fps: u32,
    pub duration: f64,
    pub frames: usize,
    pub size: (u32, u32),
    pub segments: usize,
    pub stops: usize,
    /// Frames per chunk entry and the time of each chunk's first frame.
    pub chunks: Vec<ChunkInfo>,
    /// BLAKE3 hash (hex) of every other entry.
    pub entries: std::collections::BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub entry: String,
    pub first_frame: usize,
    pub frames: usize,
    /// Time of the chunk's first frame.
    pub start: f64,
}

// ---------------------------------------------------------------------------
// Writer
// ---------------------------------------------------------------------------

/// Streams frames into a bundle file.
pub struct BundleWriter<W: Write + Seek> {
    zip: zip::ZipWriter<W>,
    tables: Tables,
    keys: EntityKeys,
    delta: DeltaEncoder,
    chunk: Writer,
    chunk_frames: usize,
    chunks: Vec<ChunkInfo>,
    frames: usize,
    times: Vec<f64>,
    digests: Vec<[u8; 32]>,
    last_time: f64,
    entries: std::collections::BTreeMap<String, String>,
    media: HashMap<blake3::Hash, String>,
    /// Recorded Lottie frames: by content, and by the scenes of the current
    /// chunk (held so their addresses stay unique).
    scenes: HashMap<blake3::Hash, u32>,
    scene_by_ptr: HashMap<usize, (Arc<vello::Scene>, u32)>,
    generator: String,
}

fn entry_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .compression_level(Some(6))
        .large_file(true)
}

/// Entries the archive keeps as they are.
fn stored_options() -> zip::write::SimpleFileOptions {
    entry_options()
        .compression_method(zip::CompressionMethod::Stored)
        .compression_level(None)
}

impl BundleWriter<std::io::BufWriter<std::fs::File>> {
    /// Create `path`, replacing it if it exists.
    pub fn create(path: &Path, generator: impl Into<String>) -> Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self::new(std::io::BufWriter::new(file), generator))
    }
}

impl<W: Write + Seek> BundleWriter<W> {
    pub fn new(output: W, generator: impl Into<String>) -> Self {
        Self {
            zip: zip::ZipWriter::new(output),
            tables: Tables::default(),
            keys: EntityKeys::default(),
            delta: DeltaEncoder::default(),
            chunk: Writer::new(),
            chunk_frames: 0,
            chunks: Vec::new(),
            frames: 0,
            times: Vec::new(),
            digests: Vec::new(),
            last_time: f64::NEG_INFINITY,
            entries: Default::default(),
            media: HashMap::new(),
            scenes: HashMap::new(),
            scene_by_ptr: HashMap::new(),
            generator: generator.into(),
        }
    }

    fn write_entry(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        // Zstandard compresses these far better than the archive's Deflate;
        // the archive only stores the frame.
        let compressed = compress(bytes)?;
        self.zip.start_file(name, stored_options())?;
        self.zip.write_all(&compressed)?;
        self.entries
            .insert(name.to_owned(), blake3::hash(bytes).to_hex().to_string());
        Ok(())
    }

    /// The key frames use for `entity`, e.g. to name the elements drawn as
    /// poll bars.
    pub fn entity_key(&mut self, entity: bevy::prelude::Entity) -> u32 {
        self.keys.key(entity)
    }

    /// Embed a media file and return its entry name. Identical files are
    /// stored once.
    pub fn add_media(&mut self, source: &Path) -> Result<String> {
        let bytes = std::fs::read(source)?;
        let hash = blake3::hash(&bytes);
        if let Some(entry) = self.media.get(&hash) {
            return Ok(entry.clone());
        }
        let extension = source
            .extension()
            .and_then(|extension| extension.to_str())
            .filter(|extension| extension.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
            .unwrap_or_default();
        let entry = format!("media/{}{extension}", &hash.to_hex()[..32]);
        // Media is usually compressed already.
        self.zip.start_file(&entry, stored_options())?;
        self.zip.write_all(&bytes)?;
        self.entries
            .insert(entry.clone(), hash.to_hex().to_string());
        self.media.insert(hash, entry.clone());
        Ok(entry)
    }

    /// Store `png` as the bundle's cover image ([`THUMBNAIL`]).
    pub fn set_thumbnail(&mut self, png: &[u8]) -> Result<()> {
        if self.entries.contains_key(THUMBNAIL) {
            return Err(BundleError::Unsupported(
                "the bundle already has a cover image".into(),
            ));
        }
        // PNG is compressed already, and readers load it in one piece.
        self.zip.start_file(THUMBNAIL, stored_options())?;
        self.zip.write_all(png)?;
        self.entries
            .insert(THUMBNAIL.to_owned(), blake3::hash(png).to_hex().to_string());
        Ok(())
    }

    /// Start another pass: its frames come in increasing time order again and
    /// may fall between the frames of earlier passes, but never on one.
    pub fn start_pass(&mut self) -> Result<()> {
        self.flush_chunk()?;
        self.last_time = f64::NEG_INFINITY;
        Ok(())
    }

    /// Store the Lottie frame `scene` once and return its index.
    fn intern_scene(&mut self, scene: &Arc<vello::Scene>) -> Result<u32> {
        let ptr = Arc::as_ptr(scene) as usize;
        if let Some((_, index)) = self.scene_by_ptr.get(&ptr) {
            return Ok(*index);
        }
        let mut w = Writer::new();
        codec::write_scene(&mut w, &mut self.tables, scene)
            .map_err(|what| BundleError::Unsupported(format!("a Lottie frame draws {what}")))?;
        let bytes = w.into_bytes();
        let hash = blake3::hash(&bytes);
        let index = match self.scenes.get(&hash) {
            Some(index) => *index,
            None => {
                let index = self.scenes.len() as u32;
                self.write_entry(&scene_entry(index), &bytes)?;
                self.scenes.insert(hash, index);
                index
            }
        };
        self.scene_by_ptr.insert(ptr, (Arc::clone(scene), index));
        Ok(index)
    }

    /// Append the next frame. Within a pass (see [`Self::start_pass`]),
    /// frames must come in increasing time order.
    /// `digest` is [`frame_digest`] of the frame as the scene drew it.
    pub fn push_frame(&mut self, frame: &Frame, digest: [u8; 32]) -> Result<()> {
        if frame.time.is_nan() || frame.time <= self.last_time {
            return Err(BundleError::Unsupported(format!(
                "frames must be recorded in increasing time order ({} after {})",
                frame.time, self.last_time
            )));
        }
        let mut lottie = HashMap::new();
        for element in std::iter::once(frame)
            .chain(&frame.motion_blur)
            .flat_map(|frame| &frame.capture.elements)
        {
            if let Some(scene) = &element.lottie {
                let index = self.intern_scene(scene)?;
                lottie.insert(Arc::as_ptr(scene) as usize, index);
            }
        }
        let lottie = |element: &CapturedElement| {
            let scene = element.lottie.as_ref()?;
            lottie.get(&(Arc::as_ptr(scene) as usize)).copied()
        };
        self.last_time = frame.time;
        self.times.push(frame.time);
        self.digests.push(digest);
        let record = FrameRecord::capture(frame, &mut self.tables, &mut self.keys, &lottie);
        if self.chunk_frames == 0 {
            self.chunks.push(ChunkInfo {
                entry: format!("frames/{:06}.bin", self.chunks.len()),
                first_frame: self.frames,
                frames: 0,
                start: frame.time,
            });
        }
        self.delta.write(&mut self.chunk, record);
        self.chunk_frames += 1;
        self.frames += 1;
        if self.chunk_frames == CHUNK_FRAMES {
            self.flush_chunk()?;
        }
        Ok(())
    }

    fn flush_chunk(&mut self) -> Result<()> {
        if self.chunk_frames == 0 {
            return Ok(());
        }
        let bytes = std::mem::take(&mut self.chunk).into_bytes();
        let info = self.chunks.last_mut().expect("chunk started");
        info.frames = self.chunk_frames;
        let entry = info.entry.clone();
        self.write_entry(&entry, &bytes)?;
        self.chunk_frames = 0;
        self.delta.reset();
        self.tables.release_unused_recipes();
        self.scene_by_ptr.clear();
        Ok(())
    }

    /// Write the tables, the static data and the manifest, and close the file.
    pub fn finish(mut self, scene: &SceneData) -> Result<W> {
        self.flush_chunk()?;
        let mut w = Writer::new();
        scene.write(&mut w, &mut self.tables);
        let scene_bytes = w.into_bytes();

        // Tables last: frames and static data may have added to them.
        let mut paths = Writer::new();
        paths.len(self.tables.paths.len());
        for path in &self.tables.paths {
            codec::write_path(&mut paths, path);
        }
        let mut images = Writer::new();
        images.len(self.tables.images.len());
        for image in &self.tables.images {
            codec::write_image_data(&mut images, image);
        }
        let mut recipes = Writer::new();
        recipes.len(self.tables.recipes.len());
        for recipe in &self.tables.recipes {
            recipes.bytes(recipe);
        }
        let mut strings = Writer::new();
        strings.len(self.tables.strings.len());
        for string in &self.tables.strings {
            strings.str(string);
        }
        self.write_entry("tables/paths.bin", &paths.into_bytes())?;
        self.write_entry("tables/images.bin", &images.into_bytes())?;
        self.write_entry("tables/recipes.bin", &recipes.into_bytes())?;
        self.write_entry("tables/strings.bin", &strings.into_bytes())?;
        let mut transitions = Writer::new();
        transitions.len(self.tables.transition_shaders.len());
        for shader in &self.tables.transition_shaders {
            model::write_transition_shader(&mut transitions, shader);
        }
        self.write_entry("tables/transitions.bin", &transitions.into_bytes())?;
        let mut effects = Writer::new();
        effects.len(self.tables.effect_shaders.len());
        for shader in &self.tables.effect_shaders {
            model::write_post_shader(&mut effects, shader);
        }
        self.write_entry("tables/effects.bin", &effects.into_bytes())?;
        self.write_entry("scene.bin", &scene_bytes)?;
        let mut index = Writer::new();
        index.len(self.times.len());
        for time in &self.times {
            index.f64(*time);
        }
        self.write_entry("index.bin", &index.into_bytes())?;
        let mut digests = Vec::with_capacity(self.digests.len() * 32);
        for digest in &self.digests {
            digests.extend_from_slice(digest);
        }
        self.write_entry("digests.bin", &digests)?;
        // A scene can take its audience without a poll: a lobby, a gate.
        if !scene.polls.is_empty() || scene.poll_session.is_some() || !scene.stop_gates.is_empty() {
            self.write_entry(POLLS, &write_polls(scene)?)?;
            let mut written = std::collections::HashSet::new();
            for image in scene.polls.iter().filter_map(|poll| poll.image.as_ref()) {
                if written.insert(image.hash.clone()) {
                    self.write_entry(&image_entry(&image.hash), &image.bytes)?;
                }
            }
        }
        if !scene.live_zones.is_empty() {
            let json = serde_json::to_vec(&scene.live_zones)
                .map_err(|error| BundleError::Corrupt(error.to_string()))?;
            self.write_entry(LIVE, &json)?;
        }
        if let Some(rehearsal) = scene
            .rehearsal
            .as_ref()
            .filter(|_| !scene.live_zones.is_empty())
        {
            let json = serde_json::to_vec(rehearsal)
                .map_err(|error| BundleError::Corrupt(error.to_string()))?;
            self.write_entry(REHEARSAL, &json)?;
        }

        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            generator: self.generator.clone(),
            title: scene.title.clone(),
            fps: scene.fps,
            duration: scene.duration,
            frames: self.frames,
            size: scene.output_size,
            segments: scene.segments.len(),
            stops: scene
                .segments
                .iter()
                .map(|segment| segment.stops.len())
                .sum(),
            chunks: self.chunks.clone(),
            entries: self.entries.clone(),
        };
        let json = serde_json::to_vec_pretty(&manifest)
            .map_err(|error| BundleError::Corrupt(error.to_string()))?;
        self.zip.start_file("manifest.json", entry_options())?;
        self.zip.write_all(&json)?;
        Ok(self.zip.finish()?)
    }
}

// ---------------------------------------------------------------------------
// Reader
// ---------------------------------------------------------------------------

/// An open bundle. Frames are decoded chunk by chunk on demand.
pub struct Bundle {
    archive: Archive,
    pub manifest: Manifest,
    pub scene: SceneData,
    tables: DecodedTables,
    /// Time of every frame, in recording order.
    times: Vec<f64>,
    /// Frame indices by increasing time.
    by_time: Vec<usize>,
    /// [`frame_digest`] of every frame as the scene drew it while recording.
    digests: Vec<[u8; 32]>,
    /// Chunks being decoded, least recently used first.
    cached: Vec<ChunkCursor>,
    /// Decoded Lottie frames of the cached chunks.
    scenes: HashMap<u32, Arc<vello::Scene>>,
}

/// Chunks a [`Bundle`] keeps decoded: the one playing, the one it left for
/// an instant off the frame grid (a later pass's chunk) or a step back, and
/// one for Presenter View's previews of other instants.
const CACHED_CHUNKS: usize = 3;

/// A chunk decoded up to the latest frame asked of it. Frames are
/// delta-encoded in order, so playback decodes one frame per step instead
/// of the whole chunk on entering it, which stalls on heavy chunks.
struct ChunkCursor {
    chunk: usize,
    /// Decompressed entry; released once every frame is decoded.
    bytes: Vec<u8>,
    /// Byte offset of the next frame in `bytes`.
    position: usize,
    decoder: DeltaDecoder,
    frames: Vec<FrameRecord>,
    /// Frames the chunk holds.
    total: usize,
}

impl ChunkCursor {
    /// Decode frames up to and including `local`, the frame's index in the chunk.
    fn decode_through(&mut self, local: usize, entry: &str) -> Result<&FrameRecord> {
        if local >= self.total {
            return Err(BundleError::Corrupt("frame index out of range".into()));
        }
        let mut r = Reader::at(&self.bytes, self.position);
        while self.frames.len() <= local {
            self.frames.push(self.decoder.read(&mut r)?);
            self.position = r.position();
        }
        if self.frames.len() == self.total && !self.bytes.is_empty() {
            if self.position != self.bytes.len() {
                return Err(BundleError::Corrupt(format!("{entry} has trailing data")));
            }
            self.bytes = Vec::new();
        }
        Ok(&self.frames[local])
    }
}

fn scene_entry(index: u32) -> String {
    format!("scenes/{index:06}.bin")
}

#[cfg(not(target_arch = "wasm32"))]
fn compress(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(zstd::bulk::compress(bytes, ZSTD_LEVEL)?)
}

#[cfg(target_arch = "wasm32")]
fn compress(bytes: &[u8]) -> Result<Vec<u8>> {
    Ok(ruzstd::encoding::compress_to_vec(
        bytes,
        ruzstd::encoding::CompressionLevel::Fastest,
    ))
}

/// Decoded with the pure-Rust decoder on every platform, so the native and
/// web players read bundles the same way.
fn decompress(name: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    let corrupt = |error: &dyn std::fmt::Display| {
        BundleError::Corrupt(format!("entry {name} could not be decompressed: {error}"))
    };
    let mut decoder =
        ruzstd::decoding::StreamingDecoder::new(bytes).map_err(|error| corrupt(&error))?;
    let mut decoded = Vec::with_capacity(bytes.len().saturating_mul(4).min(1 << 30));
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| corrupt(&error))?;
    Ok(decoded)
}

/// Whether the bundle stores `name` as a Zstandard frame.
fn zstd_entry(name: &str) -> bool {
    name != "manifest.json" && name != THUMBNAIL && !name.starts_with("media/")
}

fn read_entry(archive: &Archive, manifest: Option<&Manifest>, name: &str) -> Result<Vec<u8>> {
    let mut bytes = archive.read(name)?;
    if let Some(manifest) = manifest {
        if zstd_entry(name) {
            bytes = decompress(name, &bytes)?;
        }
        let expected = manifest
            .entries
            .get(name)
            .ok_or_else(|| BundleError::Corrupt(format!("entry {name} is not in the manifest")))?;
        if blake3::hash(&bytes).to_hex().as_str() != expected {
            return Err(BundleError::Corrupt(format!(
                "entry {name} does not match its checksum"
            )));
        }
    }
    Ok(bytes)
}

/// Entries a bundle reads as it opens: everything but frame chunks, Lottie
/// frames, media and the cover. The first chunk comes along, since a player
/// shows it next.
fn read_on_open(name: &str) -> bool {
    name == "frames/000000.bin"
        || !(name.starts_with("frames/")
            || name.starts_with("scenes/")
            || name.starts_with("media/")
            || name == THUMBNAIL)
}

impl Bundle {
    /// Open and validate a bundle file.
    pub fn open(path: &Path) -> Result<Self> {
        let bytes: Arc<[u8]> = std::fs::read(path)?.into();
        Self::from_bytes(bytes)
    }

    pub fn from_bytes(bytes: Arc<[u8]>) -> Result<Self> {
        Self::from_source(BundleSource::whole(bytes))
    }

    /// Open and validate a bundle whose bytes may still be arriving. Until
    /// the end of the file and the tables have arrived this fails with
    /// [`BundleError::Incomplete`], naming the byte ranges to add to
    /// `source` before trying again; frames then decode as their chunks
    /// arrive.
    pub fn from_source(source: BundleSource) -> Result<Self> {
        let archive = Archive::open(source)?;
        let mut missing: Vec<Range<u64>> = archive
            .spans()
            .filter(|(name, _)| read_on_open(name))
            .flat_map(|(_, span)| archive.source().missing(span))
            .collect();
        if !missing.is_empty() {
            missing.sort_by_key(|range| range.start);
            return Err(BundleError::Incomplete { missing });
        }
        let manifest: Manifest =
            serde_json::from_slice(&read_entry(&archive, None, "manifest.json")?)
                .map_err(|error| BundleError::Corrupt(format!("manifest.json: {error}")))?;
        if manifest.format != FORMAT {
            return Err(BundleError::Corrupt(format!(
                "not a Gaanim bundle (format \"{}\")",
                manifest.format
            )));
        }
        if manifest.version != VERSION {
            return Err(BundleError::UnsupportedVersion {
                found: manifest.version,
                generator: manifest.generator.clone(),
            });
        }

        let mut tables = DecodedTables {
            paths: Vec::new(),
            images: Vec::new(),
            recipes: Vec::new(),
            strings: Vec::new(),
            transition_shaders: Vec::new(),
            effect_shaders: Vec::new(),
        };
        let paths = read_entry(&archive, Some(&manifest), "tables/paths.bin")?;
        let mut r = Reader::new(&paths);
        for _ in 0..r.len()? {
            tables.paths.push(Arc::new(codec::read_path(&mut r)?));
        }
        let images = read_entry(&archive, Some(&manifest), "tables/images.bin")?;
        let mut r = Reader::new(&images);
        for _ in 0..r.len()? {
            tables.images.push(codec::read_image_data(&mut r)?);
        }
        let strings = read_entry(&archive, Some(&manifest), "tables/strings.bin")?;
        let mut r = Reader::new(&strings);
        for _ in 0..r.len()? {
            tables.strings.push(Arc::from(r.str()?));
        }
        let transitions = read_entry(&archive, Some(&manifest), "tables/transitions.bin")?;
        let mut r = Reader::new(&transitions);
        for _ in 0..r.len()? {
            tables
                .transition_shaders
                .push(Arc::new(model::read_transition_shader(&mut r)?));
        }
        let effects = read_entry(&archive, Some(&manifest), "tables/effects.bin")?;
        let mut r = Reader::new(&effects);
        for _ in 0..r.len()? {
            tables.effect_shaders.push(model::read_post_shader(&mut r)?);
        }
        let recipes = read_entry(&archive, Some(&manifest), "tables/recipes.bin")?;
        let mut r = Reader::new(&recipes);
        let mut encoded = Vec::new();
        for _ in 0..r.len()? {
            encoded.push(r.bytes()?.to_vec());
        }
        model::decode_recipes(&encoded, &mut tables)?;
        let scene_bytes = read_entry(&archive, Some(&manifest), "scene.bin")?;
        let mut scene = SceneData::read(&mut Reader::new(&scene_bytes), &tables)?;
        if manifest.entries.contains_key(POLLS) {
            let polls = read_entry(&archive, Some(&manifest), POLLS)?;
            read_polls(&polls, &mut scene, |name| {
                read_entry(&archive, Some(&manifest), name)
            })?;
        }
        if manifest.entries.contains_key(LIVE) {
            scene.live_zones =
                serde_json::from_slice(&read_entry(&archive, Some(&manifest), LIVE)?)
                    .map_err(|error| BundleError::Corrupt(format!("{LIVE}: {error}")))?;
        }
        if manifest.entries.contains_key(REHEARSAL) {
            scene.rehearsal = Some(
                serde_json::from_slice(&read_entry(&archive, Some(&manifest), REHEARSAL)?)
                    .map_err(|error| BundleError::Corrupt(format!("{REHEARSAL}: {error}")))?,
            );
        }

        let index = read_entry(&archive, Some(&manifest), "index.bin")?;
        let mut r = Reader::new(&index);
        let count = r.len()?;
        let mut times = Vec::with_capacity(count.min(1 << 24));
        for _ in 0..count {
            times.push(r.f64()?);
        }
        let chunked: usize = manifest.chunks.iter().map(|chunk| chunk.frames).sum();
        let mut by_time: Vec<usize> = (0..times.len()).collect();
        by_time.sort_by(|a, b| times[*a].total_cmp(&times[*b]));
        if times.len() != manifest.frames
            || chunked != manifest.frames
            || times.iter().any(|time| !time.is_finite())
            || by_time
                .windows(2)
                .any(|pair| times[pair[0]] >= times[pair[1]])
        {
            return Err(BundleError::Corrupt("frame index is inconsistent".into()));
        }
        let digest_bytes = read_entry(&archive, Some(&manifest), "digests.bin")?;
        if digest_bytes.len() != times.len() * 32 {
            return Err(BundleError::Corrupt(
                "frame digests are inconsistent".into(),
            ));
        }
        let digests = digest_bytes.as_chunks::<32>().0.to_vec();
        Ok(Self {
            archive,
            manifest,
            scene,
            tables,
            times,
            by_time,
            digests,
            cached: Vec::new(),
            scenes: HashMap::new(),
        })
    }

    fn open_chunk(&mut self, chunk: usize) -> Result<ChunkCursor> {
        let info = self
            .manifest
            .chunks
            .get(chunk)
            .ok_or_else(|| BundleError::Corrupt("chunk index out of range".into()))?;
        let bytes = read_entry(&self.archive, Some(&self.manifest), &info.entry)?;
        Ok(ChunkCursor {
            chunk,
            bytes,
            position: 0,
            decoder: DeltaDecoder::default(),
            frames: Vec::with_capacity(info.frames),
            total: info.frames,
        })
    }

    /// Number of recorded frames.
    pub fn frame_count(&self) -> usize {
        self.times.len()
    }

    /// Time of every recorded frame, by frame index, in recording order: a
    /// recording in two passes stores the frame grid first, then the instants
    /// between grid frames.
    pub fn times(&self) -> &[f64] {
        &self.times
    }

    /// The frame shown at `time`: the last one recorded at or before it.
    pub fn frame_index_at(&self, time: f64) -> usize {
        // Tolerate the rounding of accumulated frame steps.
        let time = time + 1e-9;
        let position = self
            .by_time
            .partition_point(|index| self.times[*index] <= time)
            .saturating_sub(1);
        self.by_time.get(position).copied().unwrap_or(0)
    }

    /// Decode frame `index`.
    pub fn frame(&mut self, index: usize) -> Result<Frame> {
        let chunk = self
            .manifest
            .chunks
            .partition_point(|info| info.first_frame <= index)
            .checked_sub(1)
            .ok_or_else(|| BundleError::Corrupt("frame index out of range".into()))?;
        let cursor = match self.cached.iter().position(|cursor| cursor.chunk == chunk) {
            Some(slot) => self.cached.remove(slot),
            None => {
                let cursor = self.open_chunk(chunk)?;
                if self.cached.len() >= CACHED_CHUNKS {
                    self.cached.remove(0);
                    self.scenes.clear();
                }
                cursor
            }
        };
        self.cached.push(cursor);
        let info = &self.manifest.chunks[chunk];
        let record = self
            .cached
            .last_mut()
            .expect("chunk cached")
            .decode_through(index - info.first_frame, &info.entry)?;
        let (archive, manifest, tables, scenes) = (
            &self.archive,
            &self.manifest,
            &self.tables,
            &mut self.scenes,
        );
        record.resolve(tables, &mut |index| {
            if let Some(scene) = scenes.get(&index) {
                return Ok(Some(Arc::clone(scene)));
            }
            let bytes = read_entry(archive, Some(manifest), &scene_entry(index))?;
            let mut r = Reader::new(&bytes);
            let scene = Arc::new(codec::read_scene(&mut r, tables)?);
            if !r.is_empty() {
                return Err(BundleError::Corrupt(
                    "a Lottie frame has trailing data".into(),
                ));
            }
            scenes.insert(index, Arc::clone(&scene));
            Ok(Some(scene))
        })
    }

    /// The bytes the bundle is read from.
    pub fn source(&self) -> &BundleSource {
        self.archive.source()
    }

    /// Byte ranges still missing to decode frame `index` and the frames of
    /// the `ahead` chunks recorded after its own, nearest first. Lottie
    /// frames are left out: a frame names them only once it decodes.
    pub fn missing_around(&self, index: usize, ahead: usize) -> Vec<Range<u64>> {
        let chunk = self
            .manifest
            .chunks
            .partition_point(|info| info.first_frame <= index)
            .saturating_sub(1);
        self.manifest
            .chunks
            .iter()
            .skip(chunk)
            .take(ahead + 1)
            .filter_map(|info| self.archive.span(&info.entry))
            .flat_map(|span| self.archive.source().missing(span))
            .collect()
    }

    /// [`frame_digest`] recorded for frame `index`.
    pub fn digest(&self, index: usize) -> Option<[u8; 32]> {
        self.digests.get(index).copied()
    }

    /// Compose every frame again and compare it with the digest recorded
    /// for it. Returns the frames that differ, by index and time.
    pub fn verify(&mut self) -> Result<Vec<(usize, f64)>> {
        let background = self.scene.background.clone();
        let mut store = FragmentStore::default();
        let mut mismatched = Vec::new();
        for index in 0..self.frame_count() {
            let frame = self.frame(index)?;
            if frame_digest(&frame, background.as_ref(), &mut store) != self.digests[index] {
                mismatched.push((index, frame.time));
            }
            store.end_frame();
        }
        Ok(mismatched)
    }

    /// The cover PNG, when the bundle has one.
    pub fn thumbnail(&mut self) -> Result<Option<Vec<u8>>> {
        if !self.manifest.entries.contains_key(THUMBNAIL) {
            return Ok(None);
        }
        read_entry(&self.archive, Some(&self.manifest), THUMBNAIL).map(Some)
    }

    /// Bytes of an embedded media entry.
    pub fn media(&mut self, entry: &str) -> Result<Vec<u8>> {
        read_entry(&self.archive, Some(&self.manifest), entry)
    }

    /// Write the embedded media into `dir` (content-addressed, so repeated
    /// extraction reuses files) and return the path of every entry.
    pub fn extract_media(&mut self, dir: &Path) -> Result<HashMap<String, PathBuf>> {
        let mut paths = HashMap::new();
        let entries: Vec<String> = self
            .manifest
            .entries
            .keys()
            .filter(|entry| entry.starts_with("media/"))
            .cloned()
            .collect();
        for entry in entries {
            let name = entry.trim_start_matches("media/");
            let path = dir.join(name);
            if !path.is_file() {
                std::fs::create_dir_all(dir)?;
                let bytes = self.media(&entry)?;
                let temporary = dir.join(format!("{name}.tmp"));
                std::fs::write(&temporary, bytes)?;
                std::fs::rename(&temporary, &path)?;
            }
            paths.insert(entry, path);
        }
        Ok(paths)
    }
}

/// Composite a frame as the preview draws it: over `background` (left out
/// in perspective), with a shader background returned as a GPU request.
/// Opacity layers are padded for the background's pixel size, as the scene
/// pads them when it renders at that size.
pub fn compose_frame(
    frame: &Frame,
    background: Option<&CanvasBackground>,
    store: &mut FragmentStore,
) -> (vello::Scene, Option<ShaderBackgroundRequest>) {
    let (composed, request) = compose_frame_layers(frame, background, store);
    (composed.flattened(), request)
}

/// [`compose_frame`] with its shader transition and the drawables under a
/// shader effect kept apart, as an export at the background's size renders
/// them.
pub fn compose_frame_layers(
    frame: &Frame,
    background: Option<&CanvasBackground>,
    store: &mut FragmentStore,
) -> (ComposedFrame, Option<ShaderBackgroundRequest>) {
    let mut request = None;
    let pixels_per_unit = background.and_then(|background| {
        gaanim_renderer::pipeline::output_pixels_per_unit(&frame.camera, background.pixel_size.0)
    });
    let composed = compose_captured_frame(
        &frame.capture,
        store,
        background.map(|background| (background, background.pixel_size)),
        pixels_per_unit,
        Some(&mut request),
        0.0,
        pixels_per_unit.unwrap_or(gaanim_renderer::pipeline::DEFAULT_EFFECT_DENSITY),
    );
    (composed, request)
}

/// Digest of what a frame hands the renderer: its composed scene, the
/// shader background it requests, its camera and its post-processing.
pub fn frame_digest(
    frame: &Frame,
    background: Option<&CanvasBackground>,
    store: &mut FragmentStore,
) -> [u8; 32] {
    let (mut composed, request) = compose_frame_layers(frame, background, store);
    let effects = std::mem::take(&mut composed.effects);
    let mut hasher = blake3::Hasher::new();
    hasher.update(&scene_digest(&composed.flattened()));
    hasher.update(&(effects.len() as u64).to_le_bytes());
    for effect in &effects {
        hasher.update(&scene_digest(&effect.scene));
        for value in effect.to_pixels.as_coeffs() {
            hasher.update(&value.to_bits().to_le_bytes());
        }
        hasher.update(&effect.image.width.to_le_bytes());
        hasher.update(&effect.image.height.to_le_bytes());
        hasher.update(&effect.request.time.to_bits().to_le_bytes());
        for (shader, values) in &effect.request.passes {
            hasher.update(blake3::hash(shader.source().as_bytes()).as_bytes());
            for value in values {
                hasher.update(&value.to_bits().to_le_bytes());
            }
        }
    }
    if let Some(request) = request {
        hasher.update(&request.time().to_bits().to_le_bytes());
        for value in request.values() {
            hasher.update(&value.to_bits().to_le_bytes());
        }
        for value in request.frame() {
            hasher.update(&value.to_bits().to_le_bytes());
        }
        hasher.update(&request.image().width.to_le_bytes());
        hasher.update(&request.image().height.to_le_bytes());
    }
    let mut camera = Writer::new();
    model::write_camera(&mut camera, &frame.camera);
    hasher.update(&camera.into_bytes());
    for pass in &frame.post {
        hasher.update(&pass.shader.to_le_bytes());
        for value in &pass.values {
            hasher.update(&value.to_bits().to_le_bytes());
        }
    }
    // A shader transition's uniforms can follow a signal: two frames at
    // the same progress may still blend differently.
    if let Some(shader) = frame
        .capture
        .transition
        .as_ref()
        .and_then(|transition| transition.shader.as_ref())
    {
        hasher.update(&shader.progress.to_bits().to_le_bytes());
        for value in &shader.values {
            hasher.update(&value.to_bits().to_le_bytes());
        }
    }
    hasher.update(&(frame.motion_blur.len() as u64).to_le_bytes());
    for sample in &frame.motion_blur {
        hasher.update(&sample.time.to_bits().to_le_bytes());
        hasher.update(&frame_digest(sample, background, store));
    }
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(time: f64) -> Frame {
        let mut camera = gaanim_math::Camera::ortho_2d(1280, 720);
        camera.position.x = time;
        Frame {
            time,
            camera,
            capture: Default::default(),
            post: Vec::new(),
            motion_blur: Vec::new(),
        }
    }

    #[test]
    fn frames_decode_in_any_order_across_passes() {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        let grid: Vec<f64> = (0..130).map(|index| f64::from(index) / 60.0).collect();
        for time in &grid {
            writer.push_frame(&frame(*time), [0; 32]).unwrap();
        }
        writer.start_pass().unwrap();
        let extras = [0.505, 1.255];
        for time in extras {
            writer.push_frame(&frame(time), [0; 32]).unwrap();
        }
        let scene = SceneData {
            fps: 60,
            duration: 130.0 / 60.0,
            ..Default::default()
        };
        let bytes = writer.finish(&scene).unwrap().into_inner();
        let mut bundle = Bundle::from_bytes(bytes.into()).unwrap();
        assert_eq!(bundle.manifest.chunks.len(), 4);

        // Playback through a grid chunk and the pass holding the extras,
        // then back and forth within and across chunks.
        let mut order: Vec<usize> = (0..40).collect();
        order.extend([130, 40, 41, 131, 75, 3, 129, 0, 59, 60, 131, 130]);
        for index in order {
            let decoded = bundle.frame(index).unwrap();
            let time = bundle.times()[index];
            assert_eq!(decoded.time, time, "frame {index}");
            assert_eq!(decoded.camera, frame(time).camera, "frame {index}");
        }
        assert!(bundle.cached.len() <= CACHED_CHUNKS);
    }

    #[test]
    fn a_bundle_opens_from_its_end_and_decodes_chunks_as_they_arrive() {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        for index in 0..200 {
            writer
                .push_frame(&frame(f64::from(index) / 60.0), [0; 32])
                .unwrap();
        }
        let scene = SceneData {
            fps: 60,
            duration: 200.0 / 60.0,
            ..Default::default()
        };
        let bytes: Arc<[u8]> = writer.finish(&scene).unwrap().into_inner().into();
        let len = bytes.len() as u64;
        let add = |source: &BundleSource, ranges: &[Range<u64>]| {
            for range in ranges {
                source.insert(
                    range.start,
                    Arc::from(&bytes[range.start as usize..range.end as usize]),
                );
            }
        };

        // From the last bytes, opening asks for the directory, then for the
        // tables and the first chunk, not the later chunks.
        let source = BundleSource::new(len);
        add(&source, std::slice::from_ref(&(len - 22..len)));
        let mut bundle = None;
        for _ in 0..4 {
            match Bundle::from_source(source.clone()) {
                Ok(opened) => {
                    bundle = Some(opened);
                    break;
                }
                Err(BundleError::Incomplete { missing }) => add(&source, &missing),
                Err(error) => panic!("{error}"),
            }
        }
        let mut bundle = bundle.expect("opens once the asked bytes arrive");
        assert!(!source.is_complete());
        bundle.frame(0).unwrap();

        let last = bundle.frame_count() - 1;
        let ahead = bundle.missing_around(last - 70, 1);
        assert!(!ahead.is_empty());
        let Err(BundleError::Incomplete { missing }) = bundle.frame(last) else {
            panic!("the last chunk has not arrived");
        };
        add(&source, &missing);
        add(&source, &ahead);
        assert_eq!(bundle.frame(last).unwrap().time, bundle.times()[last]);
        assert!(bundle.missing_around(last - 70, 1).is_empty());
    }

    #[test]
    fn a_lobby_and_its_gates_round_trip_without_polls() {
        let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
        writer.push_frame(&frame(0.0), [0; 32]).unwrap();
        let gate = StopGate {
            time: 0.5,
            until: GateCondition::Any(vec![
                GateCondition::Players { count: 5 },
                GateCondition::All(vec![
                    GateCondition::AnswerShare {
                        poll: "q0".into(),
                        share: 0.8,
                        players: true,
                    },
                    GateCondition::TimeUp { poll: "q0".into() },
                ]),
            ]),
        };
        let scene = SceneData {
            fps: 60,
            duration: 1.0 / 60.0,
            poll_session: Some(PollSessionInfo {
                relay: Some("https://relay.example.dev".into()),
                code: "ABC234".into(),
                lobby: true,
                game_segment: Some(3),
                teams: Some(gaanim_timeline::timeline::TeamsInfo {
                    names: vec!["Rojo".into(), "Azul".into()],
                    colors: vec!["#ff0000".into(), "#0000ff".into()],
                    choose: true,
                }),
                ask: Some(gaanim_timeline::timeline::AskInfo {
                    label: "Código".into(),
                    required: true,
                }),
            }),
            stop_gates: vec![gate.clone()],
            ..Default::default()
        };
        let bundle =
            Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap();
        // No poll, yet the session and its lobby are kept.
        assert!(bundle.scene.polls.is_empty());
        assert!(bundle.scene.poll_session.as_ref().unwrap().lobby);
        assert_eq!(
            bundle.scene.poll_session.as_ref().unwrap().game_segment,
            Some(3)
        );
        let teams = bundle
            .scene
            .poll_session
            .as_ref()
            .unwrap()
            .teams
            .as_ref()
            .unwrap();
        assert_eq!(teams.names, ["Rojo", "Azul"]);
        assert!(teams.choose);
        let ask = bundle
            .scene
            .poll_session
            .as_ref()
            .unwrap()
            .ask
            .as_ref()
            .unwrap();
        assert_eq!((ask.label.as_str(), ask.required), ("Código", true));
        assert_eq!(bundle.scene.stop_gates, [gate]);
    }

    #[test]
    fn polls_round_trip_in_their_own_entry() {
        let record = |polls: Vec<TimelinePoll>, scene: SceneData| {
            let mut writer = BundleWriter::new(std::io::Cursor::new(Vec::new()), "test");
            writer.push_frame(&frame(0.0), [0; 32]).unwrap();
            let scene = SceneData {
                fps: 60,
                duration: 1.0 / 60.0,
                poll_session: (!polls.is_empty()).then(|| PollSessionInfo {
                    relay: Some("https://relay.example.dev".into()),
                    code: "ABC234".into(),
                    lobby: false,
                    game_segment: None,
                    teams: None,
                    ask: None,
                }),
                polls,
                ..scene
            };
            Bundle::from_bytes(writer.finish(&scene).unwrap().into_inner().into()).unwrap()
        };
        let quiz = TimelinePoll {
            id: "q0-0badf00d".into(),
            question: "¿Cuál?".into(),
            options: vec!["A".into(), "B".into()],
            preview: vec![3, 1],
            segment: 1,
            open: 0.0,
            close: 1.0,
            quiz: Some(TimelineQuiz {
                correct: vec![0, 1],
                time: 20,
                points: 1000,
                reveal: Some(0.9),
            }),
            // Multiple choice, with a picture the bundle carries.
            multiple: true,
            image: Some(gaanim_timeline::timeline::PollImage {
                hash: "0123456789abcdef".into(),
                mime: "image/jpeg".into(),
                bytes: vec![1, 2, 3].into(),
            }),
        };
        let glyph = GlyphRecord {
            ch: 'Ñ',
            advance: 0.6,
            path: "M0 0L1 0L1 1Z".into(),
        };
        let live = SceneData {
            poll_bars: vec![PollBarRecord {
                key: 7,
                source: BarSourceRecord::Leader { rank: 1 },
                length: 4.0,
                thickness: 0.5,
                radius: 0.1,
                direction: "up".into(),
                scale: "leader".into(),
            }],
            poll_texts: vec![LiveTextRecord {
                key: 8,
                list: String::new(),
                rank: 0,
                align: "left".into(),
                glyphs: vec![glyph.clone()],
            }],
            poll_readouts: vec![LiveReadoutRecord {
                key: 9,
                source: LiveSourceRecord::Poll {
                    poll: quiz.id.clone(),
                    answers: 2,
                    measure: "percent".into(),
                    answer: 1,
                    time: 0.0,
                },
                format: ".0f".into(),
                invalid: "?".into(),
                decimal_separator: ',',
                prefix: RunRecord::default(),
                suffix: RunRecord {
                    advance: 0.5,
                    path: "M0 0L1 1".into(),
                },
                glyphs: vec![glyph],
            }],
            ..Default::default()
        };
        let bundle = record(vec![quiz.clone()], live.clone());
        assert!(bundle.manifest.entries.contains_key(POLLS));
        assert_eq!(bundle.scene.polls, [quiz]);
        assert_eq!(bundle.scene.poll_bars, live.poll_bars);
        assert_eq!(bundle.scene.poll_texts, live.poll_texts);
        assert_eq!(bundle.scene.poll_readouts, live.poll_readouts);
        assert_eq!(bundle.scene.poll_session.as_ref().unwrap().code, "ABC234");

        // A scene without polls writes no entry, as bundles did before.
        let bundle = record(Vec::new(), SceneData::default());
        assert!(!bundle.manifest.entries.contains_key(POLLS));
        assert!(bundle.scene.polls.is_empty());
        assert!(bundle.scene.poll_session.is_none());
    }
}
