use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::thread;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use gaanim_core::peniko::{Blob, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
use gaanim_scene::{RasterImage, SceneSet};
use gaanim_timeline::timeline::Timeline;
use serde::Deserialize;

pub mod analysis;
pub mod narration;

/// Metadata read from the first video stream in a media file.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub duration: f64,
    pub fps: f64,
    pub has_audio: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum VideoError {
    #[error("video file '{path}' does not exist or is not a file")]
    InvalidPath { path: PathBuf },
    #[error("could not run ffprobe; install FFmpeg and make ffprobe available in PATH: {0}")]
    ProbeUnavailable(#[source] std::io::Error),
    #[error("ffprobe rejected video '{path}': {message}")]
    ProbeFailed { path: PathBuf, message: String },
    #[error("video '{path}' has no decodable video stream")]
    NoVideoStream { path: PathBuf },
    #[error("invalid ffprobe response for '{path}': {message}")]
    InvalidMetadata { path: PathBuf, message: String },
    #[error("could not run ffmpeg; install FFmpeg and make ffmpeg available in PATH: {0}")]
    DecoderUnavailable(#[source] std::io::Error),
    #[error("ffmpeg could not decode '{path}' at {time:.3}s: {message}")]
    DecodeFailed {
        path: PathBuf,
        time: f64,
        message: String,
    },
    #[error("ffmpeg could not decode preview audio from '{path}': {message}")]
    AudioDecodeFailed { path: PathBuf, message: String },
}

/// One audio source aligned to the scene timeline for preview and export.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioTrack {
    pub path: PathBuf,
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

#[derive(Debug, thiserror::Error)]
pub enum AudioTrackError {
    #[error("audio file '{path}' does not exist or is not a file")]
    InvalidPath { path: PathBuf },
    #[error("{name} must be a finite non-negative number")]
    InvalidNumber { name: &'static str },
    #[error("fade_out requires a track duration or end")]
    FadeOutNeedsDuration,
    #[error("fade duration cannot exceed the track duration")]
    FadeExceedsDuration,
}

impl AudioTrack {
    pub fn new(
        path: impl Into<PathBuf>,
        start_time: f64,
        duration: Option<f64>,
        volume: f64,
        fade_in: f64,
        fade_out: f64,
    ) -> Result<Self, AudioTrackError> {
        let path = path.into();
        if !path.is_file() {
            return Err(AudioTrackError::InvalidPath { path });
        }
        for (name, value) in [
            ("start_time", start_time),
            ("volume", volume),
            ("fade_in", fade_in),
            ("fade_out", fade_out),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(AudioTrackError::InvalidNumber { name });
            }
        }
        if let Some(duration) = duration {
            if !duration.is_finite() || duration <= 0.0 {
                return Err(AudioTrackError::InvalidNumber { name: "duration" });
            }
            if fade_in + fade_out > duration {
                return Err(AudioTrackError::FadeExceedsDuration);
            }
        } else if fade_out > 0.0 {
            return Err(AudioTrackError::FadeOutNeedsDuration);
        }
        Ok(Self {
            path,
            start_time,
            duration,
            volume,
            fade_in,
            fade_out,
            source_offset: 0.0,
            source_duration: duration,
            speed: 1.0,
            looping: false,
        })
    }

    /// The source second this track plays at scene second `time`, or `None`
    /// while it is silent. `file_duration` is the source's length, which
    /// bounds a track without its own source duration. Matches the export
    /// mix: the output starts at `start_time`, runs `duration` (or the
    /// source to its end, once), and reads the source from
    /// `source_offset` at `speed`, wrapping when looping.
    pub fn source_time(&self, time: f64, file_duration: f64) -> Option<f64> {
        let output = time - self.start_time;
        if !output.is_finite() || output < 0.0 {
            return None;
        }
        let available = (file_duration - self.source_offset).max(0.0);
        let span = self
            .source_duration
            .map_or(available, |duration| duration.min(available));
        if span <= 0.0 {
            return None;
        }
        let speed = if self.speed > 0.0 { self.speed } else { 1.0 };
        let length = self.duration.unwrap_or(if self.looping {
            f64::INFINITY
        } else {
            span / speed
        });
        if output >= length {
            return None;
        }
        let local = output * speed;
        let local = if self.looping {
            local.rem_euclid(span)
        } else if local >= span {
            return None;
        } else {
            local
        };
        Some(self.source_offset + local)
    }

    /// The scene seconds before `until` at which this track plays the
    /// source seconds `times`, in order.
    pub fn scene_times(&self, times: &[f64], file_duration: f64, until: f64) -> Vec<f64> {
        let available = (file_duration - self.source_offset).max(0.0);
        let span = self
            .source_duration
            .map_or(available, |duration| duration.min(available));
        let speed = if self.speed > 0.0 { self.speed } else { 1.0 };
        if span <= 0.0 {
            return Vec::new();
        }
        let length = self.duration.unwrap_or(if self.looping {
            f64::INFINITY
        } else {
            span / speed
        });
        let mut sorted: Vec<f64> = times
            .iter()
            .map(|time| time - self.source_offset)
            .filter(|local| (0.0..span).contains(local))
            .collect();
        sorted.sort_by(f64::total_cmp);
        let mut scene = Vec::new();
        let mut cycle_start = 0.0;
        while cycle_start < length && self.start_time + cycle_start < until {
            for local in &sorted {
                let output = cycle_start + local / speed;
                if output >= length || self.start_time + output >= until {
                    break;
                }
                scene.push(self.start_time + output);
            }
            if !self.looping {
                break;
            }
            cycle_start += span / speed;
        }
        scene
    }

    pub fn from_media(
        path: impl Into<PathBuf>,
        start_time: f64,
        source_offset: f64,
        source_duration: f64,
        speed: f64,
        looping: bool,
        volume: f64,
    ) -> Result<Self, AudioTrackError> {
        if !source_offset.is_finite() || source_offset < 0.0 {
            return Err(AudioTrackError::InvalidNumber {
                name: "source_offset",
            });
        }
        for (name, value) in [("source_duration", source_duration), ("speed", speed)] {
            if !value.is_finite() || value <= 0.0 {
                return Err(AudioTrackError::InvalidNumber { name });
            }
        }
        let output_duration = (!looping).then_some(source_duration / speed);
        let mut track = Self::new(path, start_time, output_duration, volume, 0.0, 0.0)?;
        track.source_offset = source_offset;
        track.source_duration = Some(source_duration);
        track.speed = speed;
        track.looping = looping;
        Ok(track)
    }
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    duration: Option<String>,
    avg_frame_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    duration: Option<String>,
}

fn parse_rate(value: Option<&str>) -> Option<f64> {
    let value = value?;
    let (num, den) = value.split_once('/')?;
    let num: f64 = num.parse().ok()?;
    let den: f64 = den.parse().ok()?;
    (num.is_finite() && den.is_finite() && num > 0.0 && den > 0.0).then_some(num / den)
}

/// Inspect a local media file with ffprobe.
pub fn probe_video(path: impl AsRef<Path>) -> Result<VideoMetadata, VideoError> {
    let requested = path.as_ref();
    if !requested.is_file() {
        return Err(VideoError::InvalidPath {
            path: requested.to_path_buf(),
        });
    }
    let path = requested
        .canonicalize()
        .unwrap_or_else(|_| requested.to_path_buf());
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
        ])
        .arg(&path)
        .output()
        .map_err(VideoError::ProbeUnavailable)?;
    if !output.status.success() {
        return Err(VideoError::ProbeFailed {
            path,
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let probe: ProbeOutput =
        serde_json::from_slice(&output.stdout).map_err(|error| VideoError::InvalidMetadata {
            path: path.clone(),
            message: error.to_string(),
        })?;
    let stream = probe
        .streams
        .iter()
        .find(|stream| stream.codec_type.as_deref() == Some("video"))
        .ok_or_else(|| VideoError::NoVideoStream { path: path.clone() })?;
    let width = stream.width.filter(|value| *value > 0);
    let height = stream.height.filter(|value| *value > 0);
    let duration = stream
        .duration
        .as_deref()
        .and_then(|value| value.parse::<f64>().ok())
        .or_else(|| {
            probe
                .format
                .as_ref()?
                .duration
                .as_deref()?
                .parse::<f64>()
                .ok()
        })
        .filter(|value| value.is_finite() && *value > 0.0);
    let fps = parse_rate(stream.avg_frame_rate.as_deref()).unwrap_or(30.0);
    match (width, height, duration) {
        (Some(width), Some(height), Some(duration)) => Ok(VideoMetadata {
            width,
            height,
            duration,
            fps,
            has_audio: probe
                .streams
                .iter()
                .any(|stream| stream.codec_type.as_deref() == Some("audio")),
        }),
        _ => Err(VideoError::InvalidMetadata {
            path,
            message: "missing positive width, height, or duration".to_string(),
        }),
    }
}

/// A finite interval on the scene timeline, selecting source seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoInterval {
    pub scene_start: f64,
    pub source_start: f64,
    pub source_end: f64,
    pub speed: f64,
}
impl VideoInterval {
    pub fn scene_end(&self) -> f64 {
        self.scene_start + (self.source_end - self.source_start) / self.speed
    }
}

/// Timeline mapping and playback policy for one video drawable.
#[derive(Component, Debug, Clone)]
pub struct VideoPlayback {
    pub path: PathBuf,
    pub metadata: VideoMetadata,
    pub scene_start: f64,
    pub source_offset: f64,
    pub source_duration: f64,
    pub looping: bool,
    pub speed: f64,
    pub audio: bool,
    pub volume: f64,
    pub last_frame: Option<u64>,
    /// Inactive playback holds the constructor poster until scheduled.
    pub active: bool,
    /// Empty preserves legacy playback; otherwise intervals are sorted by scene start.
    pub intervals: Vec<VideoInterval>,
}

fn append_atempo_args(filter: &mut String, mut tempo: f64) {
    while tempo < 0.5 {
        filter.push_str(",atempo=0.5");
        tempo /= 0.5;
    }
    while tempo > 2.0 {
        filter.push_str(",atempo=2.0");
        tempo /= 2.0;
    }
    if (tempo - 1.0).abs() > 1e-9 {
        filter.push_str(&format!(",atempo={tempo:.9}"));
    }
}

/// Decode the selected embedded audio interval to WAV, preserving pitch when
/// applying the authored video speed.
fn decode_preview_audio_range(
    path: &Path,
    offset: f64,
    duration: Option<f64>,
    speed: f64,
) -> Result<Arc<[u8]>, VideoError> {
    let mut filter = format!("atrim=start={offset:.9}");
    if let Some(duration) = duration {
        filter.push_str(&format!(":duration={duration:.9}"));
    }
    filter.push_str(",asetpts=PTS-STARTPTS");
    append_atempo_args(&mut filter, speed);
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-map", "0:a:0", "-af"])
        .arg(filter)
        .args(["-f", "wav", "-acodec", "pcm_s16le", "pipe:1"])
        .output()
        .map_err(VideoError::DecoderUnavailable)?;
    if !output.status.success() || output.stdout.is_empty() {
        return Err(VideoError::AudioDecodeFailed {
            path: path.to_path_buf(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    let mut wav = output.stdout;
    fix_piped_wav_sizes(&mut wav);
    Ok(wav.into())
}

/// Write the real RIFF and `data` chunk sizes into a WAV that ffmpeg wrote to
/// a pipe. It cannot seek back to fill them in, so it leaves them at
/// 0xFFFFFFFF, and the decoder then reports a duration of hours: the
/// track would never end, keeping its player and seeking it every frame.
fn fix_piped_wav_sizes(wav: &mut [u8]) {
    if wav.len() < 12 || &wav[0..4] != b"RIFF" || &wav[8..12] != b"WAVE" {
        return;
    }
    let riff = u32::try_from(wav.len() - 8).unwrap_or(u32::MAX);
    wav[4..8].copy_from_slice(&riff.to_le_bytes());
    let mut offset = 12;
    while offset + 8 <= wav.len() {
        let size = u32::from_le_bytes(wav[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if &wav[offset..offset + 4] == b"data" {
            let data = u32::try_from(wav.len() - offset - 8).unwrap_or(u32::MAX);
            wav[offset + 4..offset + 8].copy_from_slice(&data.to_le_bytes());
            return;
        }
        // Chunks are padded to an even size.
        offset += 8 + size + (size & 1);
    }
}

pub fn decode_preview_audio(
    path: &Path,
    offset: f64,
    duration: f64,
    speed: f64,
) -> Result<Arc<[u8]>, VideoError> {
    decode_preview_audio_range(path, offset, Some(duration), speed)
}

impl VideoPlayback {
    pub fn source_time(&self, scene_time: f64) -> f64 {
        if !self.active {
            return self.source_offset;
        }
        if !self.intervals.is_empty() {
            let index = self
                .intervals
                .partition_point(|item| item.scene_start <= scene_time);
            if index == 0 {
                return self.source_offset;
            }
            let item = &self.intervals[index - 1];
            // End is exclusive. Freeze on the last source frame within the selection.
            let last =
                ((item.source_end * self.metadata.fps).ceil() - 1.0).max(0.0) / self.metadata.fps;
            return (item.source_start + (scene_time - item.scene_start).max(0.0) * item.speed)
                .min(last.max(item.source_start));
        }
        let elapsed = ((scene_time - self.scene_start).max(0.0) * self.speed).max(0.0);
        let local = if self.looping {
            elapsed.rem_euclid(self.source_duration)
        } else {
            elapsed.min(self.source_duration)
        };
        let last_frame_time = (self.metadata.duration - 1.0 / self.metadata.fps).max(0.0);
        (self.source_offset + local).min(last_frame_time)
    }

    pub fn frame_index(&self, scene_time: f64) -> u64 {
        (self.source_time(scene_time) * self.metadata.fps).floor() as u64
    }
}

pub fn decode_video_frame(
    path: &Path,
    metadata: &VideoMetadata,
    time: f64,
) -> Result<ImageData, VideoError> {
    let child = Command::new("ffmpeg")
        .args(["-v", "error", "-ss"])
        .arg(format!("{time:.9}"))
        .arg("-i")
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(VideoError::DecoderUnavailable)?;
    let output = child
        .wait_with_output()
        .map_err(VideoError::DecoderUnavailable)?;
    let expected = metadata.width as usize * metadata.height as usize * 4;
    if !output.status.success() || output.stdout.len() != expected {
        return Err(VideoError::DecodeFailed {
            path: path.to_path_buf(),
            time,
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(ImageData {
        data: Blob::from(output.stdout),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: metadata.width,
        height: metadata.height,
    })
}

#[derive(Debug)]
struct DecodeRequest {
    entity: Entity,
    generation: u64,
    path: PathBuf,
    metadata: VideoMetadata,
    frame: u64,
}

#[derive(Debug)]
struct DecodeResponse {
    entity: Entity,
    generation: u64,
    frame: u64,
    image: Result<ImageData, String>,
}

struct SequentialDecoder {
    child: Child,
    stdout: ChildStdout,
    metadata: VideoMetadata,
    next_frame: u64,
}

impl SequentialDecoder {
    fn spawn(path: &Path, metadata: &VideoMetadata) -> Result<Self, VideoError> {
        Self::spawn_at(path, metadata, 0)
    }

    fn spawn_at(
        path: &Path,
        metadata: &VideoMetadata,
        start_frame: u64,
    ) -> Result<Self, VideoError> {
        let mut command = Command::new("ffmpeg");
        command.args(["-v", "error"]);
        if start_frame > 0 {
            command
                .arg("-ss")
                .arg(format!("{:.9}", start_frame as f64 / metadata.fps));
        }
        let mut child = command
            .arg("-i")
            .arg(path)
            .args([
                "-map", "0:v:0", "-f", "rawvideo", "-pix_fmt", "rgba", "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(VideoError::DecoderUnavailable)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| VideoError::DecodeFailed {
                path: path.to_path_buf(),
                time: 0.0,
                message: "ffmpeg did not expose its raw-video pipe".to_string(),
            })?;
        Ok(Self {
            child,
            stdout,
            metadata: metadata.clone(),
            next_frame: start_frame,
        })
    }

    fn read_to(&mut self, path: &Path, frame: u64) -> Result<ImageData, VideoError> {
        let bytes = self.metadata.width as usize * self.metadata.height as usize * 4;
        let mut pixels = vec![0; bytes];
        while self.next_frame <= frame {
            self.stdout
                .read_exact(&mut pixels)
                .map_err(|error| VideoError::DecodeFailed {
                    path: path.to_path_buf(),
                    time: frame as f64 / self.metadata.fps,
                    message: error.to_string(),
                })?;
            self.next_frame += 1;
        }
        Ok(ImageData {
            data: Blob::from(pixels),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: self.metadata.width,
            height: self.metadata.height,
        })
    }
}

impl Drop for SequentialDecoder {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct RealtimeDecoderSession {
    path: PathBuf,
    decoder: SequentialDecoder,
}

const MAX_REALTIME_SEQUENTIAL_GAP: u64 = 12;

fn decode_realtime_request(
    sessions: &mut HashMap<Entity, RealtimeDecoderSession>,
    request: &DecodeRequest,
) -> (Result<ImageData, VideoError>, bool) {
    let restart = sessions.get(&request.entity).is_none_or(|session| {
        session.path != request.path
            || request.frame < session.decoder.next_frame
            || request.frame.saturating_sub(session.decoder.next_frame)
                > MAX_REALTIME_SEQUENTIAL_GAP
    });
    if restart {
        sessions.remove(&request.entity);
        match SequentialDecoder::spawn_at(&request.path, &request.metadata, request.frame) {
            Ok(decoder) => {
                sessions.insert(
                    request.entity,
                    RealtimeDecoderSession {
                        path: request.path.clone(),
                        decoder,
                    },
                );
            }
            Err(error) => return (Err(error), true),
        }
    }

    let result = sessions
        .get_mut(&request.entity)
        .expect("realtime decoder session inserted")
        .decoder
        .read_to(&request.path, request.frame);
    if result.is_err() {
        sessions.remove(&request.entity);
    }
    (result, restart)
}

/// Frame sampling policy. Exports use deterministic blocking mode; the editor
/// selects realtime mode so decoding never stalls its UI thread.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VideoSamplingMode {
    Realtime,
    #[default]
    Deterministic,
}

/// Audio tracks authored by the current canvas.
#[derive(Resource, Debug, Clone, Default)]
pub struct PreviewAudioTracks(pub Vec<AudioTrack>);

/// Enables audible timeline tracks. It is disabled in headless export apps and
/// enabled by the interactive editor host.
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct PreviewAudioEnabled(pub bool);

/// The listener's volume for the preview, on top of each track's own: a
/// level from 0 to 1, and mute. Exports ignore it.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PreviewVolume {
    pub level: f32,
    pub muted: bool,
}

impl Default for PreviewVolume {
    fn default() -> Self {
        Self {
            level: 1.0,
            muted: false,
        }
    }
}

impl PreviewVolume {
    /// What the preview's sound is multiplied by.
    pub fn gain(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.level.clamp(0.0, 1.0)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewAudioKey {
    path: PathBuf,
    offset_bits: u64,
    duration_bits: Option<u64>,
    speed_bits: u64,
    /// Size and modification time, so a take re-recorded at the same path is
    /// decoded again instead of replaying the cached previous take.
    file_stamp: Option<(u64, std::time::SystemTime)>,
}

impl PreviewAudioKey {
    fn new(track: &AudioTrack) -> Self {
        Self {
            path: track.path.clone(),
            offset_bits: track.source_offset.to_bits(),
            duration_bits: track.source_duration.map(f64::to_bits),
            speed_bits: track.speed.to_bits(),
            file_stamp: std::fs::metadata(&track.path)
                .ok()
                .and_then(|metadata| Some((metadata.len(), metadata.modified().ok()?))),
        }
    }
}

#[derive(Clone, Copy)]
struct PreviewAudioEntry {
    entity: Entity,
    duration: f64,
}

#[derive(Resource, Default)]
struct PreviewAudioRegistry {
    tracks: Vec<AudioTrack>,
    entries: Vec<Option<PreviewAudioEntry>>,
    cache: HashMap<PreviewAudioKey, Arc<[u8]>>,
    /// Seconds of each decoded source.
    durations: HashMap<PreviewAudioKey, f64>,
    failed: std::collections::HashSet<PreviewAudioKey>,
    /// Tracks whose failed seek was already reported.
    seek_warned: std::collections::HashSet<PreviewAudioKey>,
}

/// How long before its start a track gets its player, so it is ready when
/// the playhead reaches it.
const AUDIO_LEAD: f64 = 1.0;

/// Whether `track` needs a player at `scene_time`: it loops, or the playhead
/// is near or inside it. Only those tracks have one, because every player
/// mixes on the audio thread even while paused and a seek waits for that
/// thread; a scene with many short sounds would otherwise starve it (the
/// sound crackles) and stall each frame.
fn needs_player(track: &AudioTrack, active_duration: f64, scene_time: f64) -> bool {
    track.looping
        || (scene_time >= track.start_time - AUDIO_LEAD
            && scene_time < track.start_time + active_duration)
}

/// Whether a track whose source already ended must be rebuilt: it loops, or
/// the playhead is back before its end. The margin keeps a track that just
/// finished from being rebuilt while the timeline catches up with it.
fn needs_rebuild(track: &AudioTrack, active_duration: f64, scene_time: f64) -> bool {
    track.looping || scene_time < track.start_time + active_duration - 0.1
}

#[derive(Resource)]
struct VideoDecoder {
    request_tx: Sender<DecodeRequest>,
    response_rx: Receiver<DecodeResponse>,
    pending: HashMap<Entity, (u64, u64)>,
    generation: u64,
    cache: HashMap<(PathBuf, u64), ImageData>,
    lru: VecDeque<(PathBuf, u64)>,
    cache_bytes: usize,
    sequential: HashMap<PathBuf, SequentialDecoder>,
    #[cfg(test)]
    realtime_process_spawns: Arc<AtomicUsize>,
}

impl Default for VideoDecoder {
    fn default() -> Self {
        let (request_tx, request_rx) = crossbeam_channel::unbounded::<DecodeRequest>();
        let (response_tx, response_rx) = crossbeam_channel::unbounded::<DecodeResponse>();
        #[cfg(test)]
        let realtime_process_spawns = Arc::new(AtomicUsize::new(0));
        #[cfg(test)]
        let worker_process_spawns = realtime_process_spawns.clone();
        // Without threads (the web player) video frames never decode; the
        // web plays bundles, which hold no video.
        let spawned = thread::Builder::new()
            .name("gaanim-video-decoder".to_string())
            .spawn(move || {
                let mut sessions = HashMap::new();
                while let Ok(request) = request_rx.recv() {
                    let (image, _spawned) = decode_realtime_request(&mut sessions, &request);
                    #[cfg(test)]
                    if _spawned {
                        worker_process_spawns.fetch_add(1, Ordering::Relaxed);
                    }
                    let _ = response_tx.send(DecodeResponse {
                        entity: request.entity,
                        generation: request.generation,
                        frame: request.frame,
                        image: image.map_err(|error| error.to_string()),
                    });
                }
            });
        if let Err(error) = spawned {
            bevy::log::warn!("video frames are unavailable: {error}");
        }
        Self {
            request_tx,
            response_rx,
            pending: HashMap::new(),
            generation: 0,
            cache: HashMap::new(),
            lru: VecDeque::new(),
            cache_bytes: 0,
            sequential: HashMap::new(),
            #[cfg(test)]
            realtime_process_spawns,
        }
    }
}

impl VideoDecoder {
    const MAX_CACHE_BYTES: usize = 256 * 1024 * 1024;

    fn insert_cache(&mut self, key: (PathBuf, u64), image: ImageData) {
        let bytes = image.width as usize * image.height as usize * 4;
        if self.cache.insert(key.clone(), image).is_none() {
            self.cache_bytes += bytes;
            self.lru.push_back(key);
        }
        while self.cache_bytes > Self::MAX_CACHE_BYTES {
            let Some(oldest) = self.lru.pop_front() else {
                break;
            };
            if let Some(removed) = self.cache.remove(&oldest) {
                self.cache_bytes = self
                    .cache_bytes
                    .saturating_sub(removed.width as usize * removed.height as usize * 4);
            }
        }
    }

    fn decode_sequential(
        &mut self,
        path: &Path,
        metadata: &VideoMetadata,
        frame: u64,
    ) -> Result<ImageData, VideoError> {
        let restart = self
            .sequential
            .get(path)
            .is_some_and(|decoder| frame < decoder.next_frame.saturating_sub(1));
        if restart {
            self.sequential.remove(path);
        }
        if !self.sequential.contains_key(path) {
            self.sequential.insert(
                path.to_path_buf(),
                SequentialDecoder::spawn(path, metadata)?,
            );
        }
        self.sequential
            .get_mut(path)
            .expect("sequential decoder inserted")
            .read_to(path, frame)
    }
}

/// Swap a decoded frame without losing the sampling policy selected for the
/// video drawable (notably Vello 0.9's bicubic `ImageQuality::High`).
fn replace_raster_image(raster: &mut RasterImage, image: gaanim_core::peniko::ImageData) {
    let sampler = raster
        .image
        .as_ref()
        .map(|brush| brush.sampler)
        .unwrap_or_default();
    let mut brush = ImageBrush::new(image);
    brush.sampler = sampler;
    raster.image = Some(brush);
}

fn sample_video_system(world: &mut World) {
    let scene_time = world.resource::<Timeline>().current_time;
    let mode = *world.resource::<VideoSamplingMode>();
    let mut decoder = world.remove_resource::<VideoDecoder>().unwrap_or_default();

    while let Ok(response) = decoder.response_rx.try_recv() {
        if decoder.pending.get(&response.entity).copied()
            != Some((response.generation, response.frame))
        {
            continue;
        }
        decoder.pending.remove(&response.entity);
        if let Ok(image) = response.image {
            if let Some(playback) = world.get::<VideoPlayback>(response.entity) {
                decoder.insert_cache((playback.path.clone(), response.frame), image.clone());
            }
            if let Some(mut raster) = world.get_mut::<RasterImage>(response.entity) {
                replace_raster_image(&mut raster, image);
            }
            if let Some(mut playback) = world.get_mut::<VideoPlayback>(response.entity) {
                playback.last_frame = Some(response.frame);
            }
        }
    }

    let targets = world
        .query::<(Entity, &VideoPlayback)>()
        .iter(world)
        .map(|(entity, playback)| {
            (
                entity,
                playback.path.clone(),
                playback.metadata.clone(),
                playback.frame_index(scene_time),
                playback.last_frame,
            )
        })
        .collect::<Vec<_>>();

    for (entity, path, metadata, frame, last_frame) in targets {
        if last_frame == Some(frame) {
            continue;
        }
        if let Some(image) = decoder.cache.get(&(path.clone(), frame)).cloned() {
            if let Some(mut raster) = world.get_mut::<RasterImage>(entity) {
                replace_raster_image(&mut raster, image);
            }
            if let Some(mut playback) = world.get_mut::<VideoPlayback>(entity) {
                playback.last_frame = Some(frame);
            }
            continue;
        }
        match mode {
            VideoSamplingMode::Deterministic => {
                match decoder.decode_sequential(&path, &metadata, frame) {
                    Ok(image) => {
                        decoder.insert_cache((path, frame), image.clone());
                        if let Some(mut raster) = world.get_mut::<RasterImage>(entity) {
                            replace_raster_image(&mut raster, image);
                        }
                        if let Some(mut playback) = world.get_mut::<VideoPlayback>(entity) {
                            playback.last_frame = Some(frame);
                        }
                    }
                    Err(error) => gaanim_core::console::warn("video", error),
                }
            }
            VideoSamplingMode::Realtime => {
                // Keep one in-flight request per video. Superseding it every
                // editor frame makes every slower FFmpeg response stale, so a
                // busy decoder can remain on the poster forever. Once the
                // completed frame is displayed, the next update requests the
                // newest timeline position and naturally skips intermediate
                // frames without starving this entity (or other videos).
                if decoder.pending.contains_key(&entity) {
                    continue;
                }
                decoder.generation = decoder.generation.wrapping_add(1);
                let generation = decoder.generation;
                decoder.pending.insert(entity, (generation, frame));
                let _ = decoder.request_tx.send(DecodeRequest {
                    entity,
                    generation,
                    path,
                    metadata,
                    frame,
                });
            }
        }
    }
    world.insert_resource(decoder);
}

fn sync_preview_audio_system(world: &mut World) {
    use bevy::audio::{
        AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, Decodable, PlaybackSettings,
        Source, Volume,
    };

    if !world.contains_resource::<Assets<AudioSource>>() {
        return;
    }
    if !world.resource::<PreviewAudioEnabled>().0 {
        // Silence tracks that were already playing, e.g. while the editor
        // records narration over the scene.
        let entities: Vec<Entity> = world
            .get_resource::<PreviewAudioRegistry>()
            .map(|registry| {
                registry
                    .entries
                    .iter()
                    .flatten()
                    .map(|entry| entry.entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in entities {
            if let Some(sink) = world.get_mut::<AudioSink>(entity) {
                sink.pause();
            }
        }
        return;
    }

    let mut registry = world
        .remove_resource::<PreviewAudioRegistry>()
        .unwrap_or_default();
    let tracks = world.resource::<PreviewAudioTracks>().0.clone();
    if registry.tracks != tracks {
        for entry in registry.entries.drain(..).flatten() {
            let _ = world.despawn(entry.entity);
        }
        registry.tracks = tracks;
        registry.entries = vec![None; registry.tracks.len()];
    }

    let scene_time = world.resource::<Timeline>().current_time;
    for index in 0..registry.tracks.len() {
        let track = registry.tracks[index].clone();
        let key = PreviewAudioKey::new(&track);
        if registry.failed.contains(&key) {
            continue;
        }
        // Decode every source once up front (tracks share sources), so
        // giving a track its player later never waits for ffmpeg.
        let bytes = if let Some(bytes) = registry.cache.get(&key).cloned() {
            bytes
        } else {
            match decode_preview_audio_range(
                &track.path,
                track.source_offset,
                track.source_duration,
                track.speed,
            ) {
                Ok(bytes) => {
                    registry.cache.insert(key.clone(), bytes.clone());
                    bytes
                }
                Err(error) => {
                    gaanim_core::console::warn(
                        "audio",
                        format!("preview audio is unavailable: {error}"),
                    );
                    registry.failed.insert(key);
                    continue;
                }
            }
        };
        let source = AudioSource { bytes };
        let duration = match registry.durations.get(&key) {
            Some(duration) => *duration,
            None => {
                let duration = source
                    .decoder()
                    .total_duration()
                    .map(|duration| duration.as_secs_f64())
                    .filter(|duration| duration.is_finite() && *duration > 0.0);
                let Some(duration) = duration else {
                    gaanim_core::console::warn(
                        "audio",
                        format!(
                            "preview audio duration is unavailable: {}",
                            track.path.display()
                        ),
                    );
                    registry.failed.insert(key);
                    continue;
                };
                registry.durations.insert(key, duration);
                duration
            }
        };
        let active_duration = track.duration.map_or(duration, |end| end.min(duration));
        if !needs_player(&track, active_duration, scene_time) {
            if let Some(entry) = registry.entries[index].take() {
                let _ = world.despawn(entry.entity);
            }
            continue;
        }
        if registry.entries[index].is_some() {
            continue;
        }
        let handle = world.resource_mut::<Assets<AudioSource>>().add(source);
        // Play once: Bevy's looping mode wraps the source in a buffer that
        // rejects every seek, so scrubbing would leave the audio behind.
        // A finished source is rebuilt below when the playhead returns.
        let audio = world
            .spawn((
                AudioPlayer(handle),
                PlaybackSettings::ONCE
                    .paused()
                    .with_volume(Volume::Linear(track.volume as f32)),
            ))
            .id();
        registry.entries[index] = Some(PreviewAudioEntry {
            entity: audio,
            duration,
        });
    }

    let listener = world
        .get_resource::<PreviewVolume>()
        .map_or(1.0, PreviewVolume::gain);
    let timeline = world.resource::<Timeline>();
    let timeline_playing = timeline.is_playing;
    let playback_rate = timeline.playback_rate.max(0.01);
    let mut rebuild = Vec::new();
    let mut seek_errors = Vec::new();
    for (index, entry) in registry.entries.iter().copied().enumerate() {
        let Some(entry) = entry else {
            continue;
        };
        let track = &registry.tracks[index];
        let elapsed = (scene_time - track.start_time).max(0.0);
        let active_duration = track
            .duration
            .map(|duration| duration.min(entry.duration))
            .unwrap_or(entry.duration);
        let active = scene_time >= track.start_time && (track.looping || elapsed < active_duration);
        let target = if track.looping {
            elapsed.rem_euclid(entry.duration)
        } else {
            elapsed.min((entry.duration - 1e-6).max(0.0))
        };
        let Some(mut sink) = world.get_mut::<AudioSink>(entry.entity) else {
            continue;
        };
        // A source that played to its end empties the sink, which can no
        // longer seek. Rebuild it while the playhead can still reach it.
        if sink.empty() {
            if needs_rebuild(track, active_duration, scene_time) {
                rebuild.push(index);
            }
            continue;
        }
        let fade_in = if track.fade_in > 0.0 {
            (elapsed / track.fade_in).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let fade_out = if track.fade_out > 0.0 {
            ((active_duration - elapsed) / track.fade_out).clamp(0.0, 1.0)
        } else {
            1.0
        };
        sink.set_volume(Volume::Linear(
            (track.volume * fade_in.min(fade_out)) as f32 * listener,
        ));
        sink.set_speed(playback_rate as f32);
        // The sink measures and seeks in playback time; the decoded track is
        // in scene time, which runs `playback_rate` times faster.
        let observed = sink.position().as_secs_f64() * playback_rate;
        if (observed - target).abs() > 0.05
            && let Err(error) =
                sink.try_seek(std::time::Duration::from_secs_f64(target / playback_rate))
        {
            seek_errors.push((index, error.to_string()));
        }
        if active && timeline_playing {
            sink.play();
        } else {
            sink.pause();
        }
    }
    for (index, error) in seek_errors {
        let key = PreviewAudioKey::new(&registry.tracks[index]);
        if registry.seek_warned.insert(key) {
            gaanim_core::console::warn(
                "audio",
                format!(
                    "preview audio cannot follow the playhead in {}: {error}",
                    registry.tracks[index].path.display()
                ),
            );
        }
    }
    for index in rebuild {
        if let Some(entry) = registry.entries[index].take() {
            let _ = world.despawn(entry.entity);
        }
    }
    world.insert_resource(registry);
}

pub struct GaanimMediaPlugin;

impl Plugin for GaanimMediaPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<VideoSamplingMode>()
            .init_resource::<PreviewAudioEnabled>()
            .init_resource::<PreviewVolume>()
            .init_resource::<PreviewAudioTracks>()
            .init_resource::<VideoDecoder>()
            .init_resource::<PreviewAudioRegistry>()
            .add_systems(
                Update,
                (sample_video_system, sync_preview_audio_system)
                    .chain()
                    .in_set(SceneSet::Updaters),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        start: f64,
        duration: Option<f64>,
        offset: f64,
        speed: f64,
        looping: bool,
    ) -> AudioTrack {
        AudioTrack {
            path: PathBuf::from("x.wav"),
            start_time: start,
            duration,
            volume: 1.0,
            fade_in: 0.0,
            fade_out: 0.0,
            source_offset: offset,
            source_duration: None,
            speed,
            looping,
        }
    }

    #[test]
    fn source_time_follows_start_offset_speed_and_loops() {
        let plain = track(2.0, None, 0.0, 1.0, false);
        assert_eq!(plain.source_time(1.9, 10.0), None);
        assert_eq!(plain.source_time(2.5, 10.0), Some(0.5));
        assert_eq!(plain.source_time(12.0, 10.0), None);
        let trimmed = track(0.0, Some(3.0), 1.0, 2.0, false);
        assert_eq!(trimmed.source_time(1.0, 10.0), Some(3.0));
        assert_eq!(trimmed.source_time(3.0, 10.0), None);
        let looped = track(0.0, Some(9.0), 0.0, 1.0, true);
        assert_eq!(looped.source_time(4.5, 4.0), Some(0.5));
        assert_eq!(looped.source_time(9.0, 4.0), None);
    }

    #[test]
    fn scene_times_map_onsets_through_the_track() {
        let looped = track(1.0, Some(7.0), 0.0, 2.0, true);
        // A 4 s source at double speed loops every 2 scene seconds.
        assert_eq!(
            looped.scene_times(&[0.0, 3.0], 4.0, 100.0),
            vec![1.0, 2.5, 3.0, 4.5, 5.0, 6.5, 7.0]
        );
        let endless = track(0.0, None, 0.0, 1.0, true);
        assert_eq!(endless.scene_times(&[1.0], 2.0, 6.0), vec![1.0, 3.0, 5.0]);
        let offset = track(0.0, None, 1.0, 1.0, false);
        assert_eq!(
            offset.scene_times(&[0.5, 1.5, 2.0], 3.0, 100.0),
            vec![0.5, 1.0]
        );
    }

    fn image_data(rgba: [u8; 4]) -> ImageData {
        ImageData {
            data: Blob::from(rgba.to_vec()),
            format: ImageFormat::Rgba8,
            alpha_type: ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        }
    }

    fn playback(looping: bool, speed: f64) -> VideoPlayback {
        VideoPlayback {
            path: "clip.mp4".into(),
            metadata: VideoMetadata {
                width: 10,
                height: 10,
                duration: 12.0,
                fps: 30.0,
                has_audio: true,
            },
            scene_start: 2.0,
            source_offset: 3.0,
            source_duration: 4.0,
            looping,
            speed,
            audio: true,
            volume: 1.0,
            last_frame: None,
            active: true,
            intervals: Vec::new(),
        }
    }

    #[test]
    fn media_video_declaration_holds_poster_until_activation() {
        let mut video = playback(false, 2.0);
        video.active = false;
        assert_eq!(video.source_time(100.0), 3.0);
        video.active = true;
        assert_eq!(video.source_time(100.0), 7.0);
    }

    #[test]
    fn media_segments_sample_absolute_time_and_exclusive_ends() {
        let mut video = playback(true, 1.0);
        video.intervals = vec![
            VideoInterval {
                scene_start: 1.0,
                source_start: 2.0,
                source_end: 4.0,
                speed: 2.0,
            },
            VideoInterval {
                scene_start: 3.0,
                source_start: 6.0,
                source_end: 8.0,
                speed: 1.0,
            },
        ];
        for (time, expected) in [
            (0.0, 90),
            (1.0, 60),
            (1.5, 90),
            (2.0, 119),
            (2.9, 119),
            (3.0, 180),
            (4.0, 210),
            (10.0, 239),
            (1.0, 60),
            (0.0, 90),
        ] {
            assert_eq!(video.frame_index(time), expected, "time {time}");
        }
    }

    #[test]
    fn non_looping_video_clamps_before_and_after_playback() {
        let playback = playback(false, 2.0);
        assert_eq!(playback.source_time(0.0), 3.0);
        assert_eq!(playback.source_time(3.0), 5.0);
        assert_eq!(playback.source_time(20.0), 7.0);
    }

    #[test]
    fn looping_video_wraps_selected_source_range() {
        let playback = playback(true, 1.0);
        assert_eq!(playback.source_time(2.0), 3.0);
        assert_eq!(playback.source_time(7.0), 4.0);
    }

    #[test]
    fn parses_fractional_ffprobe_frame_rates() {
        assert!((parse_rate(Some("30000/1001")).unwrap() - 29.970_029_97).abs() < 1e-6);
        assert_eq!(parse_rate(Some("0/0")), None);
    }

    #[test]
    fn realtime_sampling_displays_a_completed_frame_when_the_timeline_advances() {
        let (request_tx, request_rx) = crossbeam_channel::unbounded();
        let (response_tx, response_rx) = crossbeam_channel::unbounded();
        let decoder = VideoDecoder {
            request_tx,
            response_rx,
            pending: HashMap::new(),
            generation: 0,
            cache: HashMap::new(),
            lru: VecDeque::new(),
            cache_bytes: 0,
            sequential: HashMap::new(),
            realtime_process_spawns: Arc::new(AtomicUsize::new(0)),
        };
        let mut world = World::new();
        world.insert_resource(Timeline::new());
        world.insert_resource(VideoSamplingMode::Realtime);
        world.insert_resource(decoder);
        let entity = world
            .spawn((gaanim_scene::RasterImage::none(), playback(false, 1.0)))
            .id();

        sample_video_system(&mut world);
        let first = request_rx.recv().expect("initial frame request");
        world.resource_mut::<Timeline>().current_time = 3.0;
        sample_video_system(&mut world);

        response_tx
            .send(DecodeResponse {
                entity,
                generation: first.generation,
                frame: first.frame,
                image: Ok(image_data([255, 0, 0, 255])),
            })
            .unwrap();
        sample_video_system(&mut world);

        assert_eq!(
            world.get::<VideoPlayback>(entity).unwrap().last_frame,
            Some(90)
        );
        assert!(world.get::<RasterImage>(entity).unwrap().image.is_some());
    }

    #[test]
    fn realtime_decoder_reuses_ffmpeg_for_adjacent_frames() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            eprintln!("skipping realtime decoder check because FFmpeg is unavailable");
            return;
        }
        let path = std::env::temp_dir().join(format!(
            "gaanim-realtime-decoder-test-{}.mp4",
            std::process::id()
        ));
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=16x12:r=8:d=1",
                "-c:v",
                "mpeg4",
            ])
            .arg(&path)
            .status()
            .expect("ffmpeg was available during the initial probe");
        assert!(status.success());
        let metadata = probe_video(&path).expect("fixture should be probeable");
        let decoder = VideoDecoder::default();

        for frame in [0, 1] {
            decoder
                .request_tx
                .send(DecodeRequest {
                    entity: Entity::from_bits(1),
                    generation: frame + 1,
                    path: path.clone(),
                    metadata: metadata.clone(),
                    frame,
                })
                .unwrap();
            decoder
                .response_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .expect("decoded frame response");
        }

        assert_eq!(
            decoder.realtime_process_spawns.load(Ordering::Relaxed),
            1,
            "adjacent realtime frames should share one FFmpeg process"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn probes_and_decodes_a_real_mp4_when_ffmpeg_is_available() {
        if Command::new("ffmpeg").arg("-version").output().is_err()
            || Command::new("ffprobe").arg("-version").output().is_err()
        {
            eprintln!("skipping MP4 integration check because FFmpeg is unavailable");
            return;
        }
        let path =
            std::env::temp_dir().join(format!("gaanim-media-test-{}.mp4", std::process::id()));
        let status = Command::new("ffmpeg")
            .args([
                "-y",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=16x12:r=4:d=0.5",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:duration=0.5",
                "-shortest",
                "-c:v",
                "mpeg4",
                "-c:a",
                "aac",
            ])
            .arg(&path)
            .status()
            .expect("ffmpeg was available during the initial probe");
        assert!(status.success());

        let metadata = probe_video(&path).expect("fixture should be probeable");
        assert_eq!((metadata.width, metadata.height), (16, 12));
        assert!(metadata.has_audio);
        let frame = decode_video_frame(&path, &metadata, 0.25).expect("frame should decode");
        assert_eq!(frame.data.data().len(), 16 * 12 * 4);
        let audio =
            decode_preview_audio(&path, 0.0, 0.25, 1.25).expect("embedded audio should decode");
        assert!(audio.len() > 44);
        let source = bevy::audio::AudioSource { bytes: audio };
        assert!(
            bevy::audio::Decodable::decoder(&source).next().is_some(),
            "Bevy must enable the WAV decoder used by preview audio"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn finished_tracks_are_rebuilt_only_while_the_playhead_can_reach_them() {
        let path = std::env::temp_dir().join(format!("gaanim-rebuild-{}.wav", std::process::id()));
        narration::write_wav_take(&path, &[0.0; 8], 8_000).unwrap();
        let mut track = AudioTrack::new(&path, 2.0, Some(3.0), 1.0, 0.0, 0.0).unwrap();
        assert!(
            needs_rebuild(&track, 3.0, 0.5),
            "scrubbed back before the track"
        );
        assert!(
            needs_rebuild(&track, 3.0, 4.0),
            "scrubbed back inside the track"
        );
        assert!(!needs_rebuild(&track, 3.0, 4.95), "just finished playing");
        assert!(!needs_rebuild(&track, 3.0, 9.0), "after the track");
        track.looping = true;
        assert!(needs_rebuild(&track, 3.0, 9.0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn only_tracks_near_the_playhead_get_a_player() {
        let path = std::env::temp_dir().join(format!("gaanim-player-{}.wav", std::process::id()));
        narration::write_wav_take(&path, &[0.0; 8], 8_000).unwrap();
        let mut track = AudioTrack::new(&path, 10.0, Some(0.5), 1.0, 0.0, 0.0).unwrap();
        assert!(!needs_player(&track, 0.5, 2.0), "long before the sound");
        assert!(
            needs_player(&track, 0.5, 9.5),
            "just before it, to be ready"
        );
        assert!(needs_player(&track, 0.5, 10.2), "while it sounds");
        assert!(!needs_player(&track, 0.5, 10.6), "after it");
        track.looping = true;
        assert!(needs_player(&track, 0.5, 2.0));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn decoded_preview_audio_seeks_both_ways_when_ffmpeg_is_available() {
        use bevy::audio::{Decodable, Source};
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            eprintln!("skipping preview seek check because FFmpeg is unavailable");
            return;
        }
        // One level per second, so a sample tells where the decoder is.
        let directory = std::env::temp_dir().join(format!("gaanim-seek-{}", std::process::id()));
        let path = directory.join("take.wav");
        let levels = [0.1_f32, 0.4, 0.7];
        let samples: Vec<f32> = levels
            .iter()
            .flat_map(|level| std::iter::repeat_n(*level, 8_000))
            .collect();
        narration::write_wav_take(&path, &samples, 8_000).unwrap();
        let bytes = decode_preview_audio_range(&path, 0.0, None, 1.0).unwrap();
        let mut decoder = bevy::audio::AudioSource { bytes }.decoder();
        // A track must end when its sound does, not hours later.
        let duration = decoder.total_duration().expect("a known duration");
        assert!((duration.as_secs_f64() - 3.0).abs() < 0.05, "{duration:?}");
        for (second, level) in [(2.5, 0.7), (0.5, 0.1), (1.5, 0.4)] {
            decoder
                .try_seek(std::time::Duration::from_secs_f64(second))
                .expect("preview audio must seek");
            let sample = decoder.next().expect("audio after the seek");
            assert!(
                (sample - level).abs() < 0.02,
                "{second} s: {sample} != {level}"
            );
        }
        let _ = std::fs::remove_dir_all(directory);
    }
}
