//! The preview's control channel, `--listen`: an editor moves the preview
//! and hears where it is, one JSON object per line.
//!
//! Commands come in (`seek`, `follow`, `play`, `pause`, `loop`, `unloop`)
//! and events go out
//! (`ready`, `reload`, `failed`, `playhead`, `error`). With `--listen stdio`
//! they use the process's stdin and stdout; with `--listen 127.0.0.1:PORT`
//! every editor that connects to that loopback port. Without `--listen`
//! nothing listens.
//!
//! Each `ready` carries the scene's timeline, the object `gaanim inspect
//! --json` prints, so an editor knows the segments, stops and their lines
//! without running the script again.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use crossbeam_channel::{Receiver, Sender};
use gaanim_api::canvas::TimelineIndex;
use gaanim_editor::{EditorState, PresentationMode};
use gaanim_timeline::timeline::{SegmentMetadata, Timeline};
use serde::Deserialize;
use serde_json::{Value, json};

/// Where `--listen` takes commands and sends events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListenAddress {
    /// The process's stdin and stdout.
    Stdio,
    /// A TCP port on a loopback address.
    Tcp(SocketAddr),
}

impl ListenAddress {
    /// `stdio`, or a loopback `HOST:PORT` (`127.0.0.1:7801`, `[::1]:7801`,
    /// `localhost:7801`).
    pub fn parse(value: &str) -> Result<Self, String> {
        let value = value.trim();
        if value == "stdio" {
            return Ok(Self::Stdio);
        }
        let address = match value.strip_prefix("localhost:") {
            Some(port) => format!("127.0.0.1:{port}"),
            None => value.to_string(),
        };
        let address: SocketAddr = address.parse().map_err(|_| {
            format!(
                "--listen takes stdio or a loopback address such as 127.0.0.1:7801, not `{value}`"
            )
        })?;
        if !address.ip().is_loopback() {
            return Err(format!(
                "--listen only listens on this computer (127.0.0.1 or [::1]), not on {}",
                address.ip()
            ));
        }
        Ok(Self::Tcp(address))
    }
}

/// A command an editor sends.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Rest at a time, a segment or `Section` (its first stop, else its
    /// end), or a stop (in `segment` when given).
    Seek {
        time: Option<f64>,
        segment: Option<String>,
        stop: Option<String>,
    },
    /// Rest at the instant line `line` of `file` stands for (see
    /// [`TimelineIndex::instant_at`]).
    Follow {
        file: PathBuf,
        line: u32,
    },
    Play,
    Pause,
    /// Repeat a segment or `Section` (from its first segment's start to
    /// its last's end), or `start`..`end`, as the preview's loop does.
    Loop {
        segment: Option<String>,
        start: Option<f64>,
        end: Option<f64>,
    },
    /// End the loop, restoring the range it replaced.
    Unloop,
}

/// What an editor asked to loop, resolved again after each reload.
#[derive(Debug, Clone, PartialEq)]
enum LoopRequest {
    Segment(String),
    Range(f64, f64),
}

/// A command, or why its line could not be read.
type Incoming = Result<Command, String>;

/// The channel's two ends inside the app, and what it knows of the scene.
#[derive(Resource)]
pub struct ControlChannel {
    commands: Receiver<Incoming>,
    events: Sender<String>,
    /// The index of the scene shown, for `follow` and `ready`.
    index: Option<TimelineIndex>,
    /// The script previewed, absolute, as `ready`'s timeline names it.
    script: Option<PathBuf>,
    last_playhead: Option<Playhead>,
    last_sent: Option<Instant>,
    /// The loop an editor asked for, while the preview keeps it.
    loop_request: Option<LoopRequest>,
    /// The range that request looped, to notice the preview changing it.
    applied_loop: Option<(f64, f64)>,
    /// A new scene arrived since the commands last ran.
    reloaded: bool,
}

impl ControlChannel {
    /// Send `event` to every listening editor.
    pub fn send(&self, event: Value) {
        let _ = self.events.send(event.to_string());
    }

    /// A new scene arrived, with `index`, and is being rebuilt.
    pub fn reloading(&mut self, index: TimelineIndex) {
        self.index = Some(index);
        self.reloaded = true;
        self.send(json!({ "event": "reload" }));
    }

    /// The scene is shown.
    pub fn ready(&mut self, duration: f64) {
        self.last_playhead = None;
        self.send(self.ready_event(duration));
    }

    /// `ready`, with the timeline of the scene shown; null for a bundle,
    /// which runs no script.
    fn ready_event(&self, duration: f64) -> Value {
        let timeline = self.index.as_ref().map(|index| {
            crate::inspect::index_json(self.script.as_deref().unwrap_or(Path::new("")), index)
        });
        json!({ "event": "ready", "duration": duration, "timeline": timeline })
    }

    /// The script failed with `message`.
    pub fn failed(&self, message: &str) {
        let (summary, location) = gaanim_core::console::traceback_summary(message);
        self.send(json!({
            "event": "failed",
            "message": summary,
            "file": location.as_ref().map(|location| location.file.to_string_lossy()),
            "line": location.as_ref().map(|location| location.line),
        }));
    }
}

/// Start listening at `address`, inside `world`'s app previewing `script`.
pub fn listen(
    world: &mut World,
    address: ListenAddress,
    script: Option<&Path>,
) -> Result<(), String> {
    let (command_tx, command_rx) = crossbeam_channel::unbounded::<Incoming>();
    let (event_tx, event_rx) = crossbeam_channel::unbounded::<String>();
    match address {
        ListenAddress::Stdio => {
            spawn("gaanim-control-in", move || {
                for line in std::io::stdin().lock().lines() {
                    let Ok(line) = line else { break };
                    if let Some(incoming) = parse_line(&line)
                        && command_tx.send(incoming).is_err()
                    {
                        break;
                    }
                }
            })?;
            spawn("gaanim-control-out", move || {
                for event in event_rx {
                    let mut stdout = std::io::stdout().lock();
                    if writeln!(stdout, "{event}")
                        .and_then(|()| stdout.flush())
                        .is_err()
                    {
                        break;
                    }
                }
            })?;
        }
        ListenAddress::Tcp(address) => {
            let listener = TcpListener::bind(address)
                .map_err(|error| format!("--listen {address}: {error}"))?;
            let clients: Arc<Mutex<Vec<TcpStream>>> = Arc::default();
            // A new editor hears the latest `ready` and `playhead` at once.
            let latest: Arc<Mutex<Vec<String>>> = Arc::default();
            let (accepted, latest_for_clients) = (clients.clone(), latest.clone());
            spawn("gaanim-control-accept", move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    let Ok(reader) = stream.try_clone() else {
                        continue;
                    };
                    let mut writer = stream;
                    let mut caught_up = true;
                    for event in latest_for_clients.lock().expect("control events").iter() {
                        caught_up &= writeln!(writer, "{event}").is_ok();
                    }
                    if caught_up {
                        accepted.lock().expect("control clients").push(writer);
                    }
                    let commands = command_tx.clone();
                    let _ = spawn("gaanim-control-client", move || {
                        for line in BufReader::new(reader).lines() {
                            let Ok(line) = line else { break };
                            if let Some(incoming) = parse_line(&line)
                                && commands.send(incoming).is_err()
                            {
                                break;
                            }
                        }
                    });
                }
            })?;
            spawn("gaanim-control-out", move || {
                for event in event_rx {
                    remember(&mut latest.lock().expect("control events"), &event);
                    clients
                        .lock()
                        .expect("control clients")
                        .retain_mut(|client| writeln!(client, "{event}").is_ok());
                }
            })?;
            gaanim_core::console::info("listen", format!("editors connect to {address}"));
        }
    }
    let commands = gaanim_editor::idle::waking_receiver(world, command_rx, "gaanim-control-wake");
    world.insert_resource(ControlChannel {
        commands,
        events: event_tx,
        index: None,
        // Absolute and written as the script lines are, like `inspect`.
        script: script.map(|script| {
            std::path::absolute(script)
                .map(|script| gaanim_core::console::plain_path(&script))
                .unwrap_or_else(|_| script.to_path_buf())
        }),
        last_playhead: None,
        last_sent: None,
        loop_request: None,
        applied_loop: None,
        reloaded: false,
    });
    Ok(())
}

fn spawn(name: &str, body: impl FnOnce() + Send + 'static) -> Result<(), String> {
    std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body)
        .map(|_| ())
        .map_err(|error| format!("--listen: {error}"))
}

/// Keep `event` as the latest of its kind when a new editor should hear it.
fn remember(latest: &mut Vec<String>, event: &str) {
    let kind = |event: &str| {
        serde_json::from_str::<Value>(event)
            .ok()
            .and_then(|value| value["event"].as_str().map(str::to_owned))
    };
    let Some(new) = kind(event).filter(|kind| kind == "ready" || kind == "playhead") else {
        return;
    };
    latest.retain(|known| kind(known).as_deref() != Some(new.as_str()));
    latest.push(event.to_string());
}

/// The command on `line`; blank lines carry none.
fn parse_line(line: &str) -> Option<Incoming> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    Some(serde_json::from_str(line).map_err(|error| format!("{error}: {line}")))
}

/// The time `command`'s seek names on `timeline`.
/// Whether `left` and `right` name the same thing, ignoring case.
fn same_name(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.trim().to_lowercase()
}

/// The segments `name` selects, in order: a segment by name, or every
/// segment of a `Section` by its key; every segment without a name.
fn named_segments<'a>(timeline: &'a Timeline, name: Option<&str>) -> Vec<&'a SegmentMetadata> {
    timeline
        .segments
        .iter()
        .filter(|candidate| {
            name.is_none_or(|name| {
                same_name(&candidate.name, name)
                    || gaanim_timeline::selection::section_key(&candidate.name)
                        .is_some_and(|key| same_name(key, name))
            })
        })
        .collect()
}

fn seek_time(
    timeline: &Timeline,
    time: Option<f64>,
    segment: Option<&str>,
    stop: Option<&str>,
) -> Result<f64, String> {
    let same = same_name;
    match (time, segment, stop) {
        (Some(time), None, None) if time.is_finite() && time >= 0.0 => {
            Ok(time.min(timeline.cached_duration.max(0.0)))
        }
        (Some(_), None, None) => Err("seek: time must be a non-negative number".to_string()),
        (None, Some(_), _) | (None, None, Some(_)) => {
            let segments = named_segments(timeline, segment);
            if segments.is_empty() {
                return Err(format!(
                    "seek: no segment or section named {:?}",
                    segment.unwrap_or_default()
                ));
            }
            match stop {
                // Where it first rests, as `gaanim export --frame` shows it:
                // its start may still show the slide before.
                None => Ok(segments
                    .iter()
                    .find_map(|segment| segment.stops.first())
                    .map_or(segments[0].end_time, |stop| stop.time)),
                Some(stop) => segments
                    .iter()
                    .flat_map(|segment| &segment.stops)
                    .find(|candidate| {
                        candidate
                            .name
                            .as_deref()
                            .is_some_and(|name| same(name, stop))
                    })
                    .map(|stop| stop.time)
                    .ok_or_else(|| format!("seek: no stop named {stop:?}")),
            }
        }
        _ => Err("seek takes time, or segment and an optional stop, or stop".to_string()),
    }
}

/// What a `loop` command asks for.
fn loop_request(
    segment: Option<String>,
    start: Option<f64>,
    end: Option<f64>,
) -> Result<LoopRequest, String> {
    match (segment, start, end) {
        (Some(segment), None, None) => Ok(LoopRequest::Segment(segment)),
        (None, Some(start), Some(end)) => Ok(LoopRequest::Range(start, end)),
        _ => Err("loop takes segment, or start and end".to_string()),
    }
}

/// The range `request` loops on `timeline`.
fn loop_range(timeline: &Timeline, request: &LoopRequest) -> Result<(f64, f64), String> {
    let duration = timeline.cached_duration.max(0.0);
    match request {
        LoopRequest::Segment(name) => {
            let segments = named_segments(timeline, Some(name));
            match (segments.first(), segments.last()) {
                (Some(first), Some(last)) => Ok((first.start_time, last.end_time)),
                _ => Err(format!("loop: no segment or section named {name:?}")),
            }
        }
        LoopRequest::Range(start, end) => {
            let valid = start.is_finite() && end.is_finite() && *start >= 0.0 && end > start;
            if !valid || *start >= duration {
                return Err(format!(
                    "loop: start and end must be seconds with 0 <= start < end and start before \
                     the end of the scene ({duration}s), not {start}..{end}"
                ));
            }
            Ok((*start, end.min(duration)))
        }
    }
}

/// Where the preview rests, as `playhead` reports it.
#[derive(Debug, Clone, PartialEq)]
struct Playhead {
    /// Milliseconds, so a still playhead sends nothing.
    millis: i64,
    segment: Option<String>,
    stop: Option<String>,
    playing: bool,
    /// The loop's start and end in milliseconds, while a loop is on.
    looped: Option<(i64, i64)>,
}

impl Playhead {
    /// Where `timeline` rests; `looping` says whether the preview's loop
    /// is on.
    fn of(timeline: &Timeline, looping: bool) -> Self {
        let time = timeline.current_time;
        let position = timeline.segment_position_at(time);
        let segment = position.and_then(|position| {
            timeline
                .segments
                .iter()
                .find(|segment| segment.id == position.segment_id)
        });
        let stop = segment
            .zip(position.and_then(|position| position.stop_index))
            .and_then(|(segment, index)| segment.stops.get(index))
            .map(|stop| stop.name.clone().unwrap_or_default());
        let millis = |time: f64| (time * 1000.0).round() as i64;
        Self {
            millis: millis(time),
            segment: segment.map(|segment| segment.name.clone()),
            stop,
            playing: timeline.is_playing,
            looped: timeline
                .loop_range
                .filter(|_| looping)
                .map(|(start, end)| (millis(start), millis(end))),
        }
    }

    fn event(&self) -> Value {
        json!({
            "event": "playhead",
            "time": self.millis as f64 / 1000.0,
            "segment": self.segment,
            "stop": self.stop,
            "playing": self.playing,
            "loop": self.looped.map(|(start, end)| [start as f64 / 1000.0, end as f64 / 1000.0]),
        })
    }
}

/// While playing, the playhead is reported at most this often.
const PLAYHEAD_INTERVAL: Duration = Duration::from_millis(100);

/// The range the preview loops, while its loop is on.
fn active_loop(timeline: &Timeline, editor: Option<&EditorState>) -> Option<(f64, f64)> {
    timeline
        .loop_range
        .filter(|_| editor.is_some_and(EditorState::segment_loop_active))
}

/// Keep the loop an editor asked for across a reload, which re-resolves
/// the preview's loop as the slide at the playhead; forget it once the
/// preview's own loop controls change it.
fn keep_requested_loop(
    channel: &mut ControlChannel,
    timeline: &mut Timeline,
    editor: Option<&mut EditorState>,
) {
    let Some(editor) = editor else {
        return;
    };
    let reloaded = std::mem::take(&mut channel.reloaded);
    let Some(request) = channel.loop_request.clone() else {
        return;
    };
    if !reloaded {
        if active_loop(timeline, Some(editor)) != channel.applied_loop {
            channel.loop_request = None;
            channel.applied_loop = None;
        }
        return;
    }
    match loop_range(timeline, &request) {
        Ok(range) if editor.set_segment_loop(timeline, range, false).is_ok() => {
            channel.applied_loop = active_loop(timeline, Some(editor));
        }
        Ok(_) => {}
        Err(message) => {
            editor.clear_segment_loop(timeline);
            channel.loop_request = None;
            channel.applied_loop = None;
            channel.send(json!({ "event": "error", "message": message }));
        }
    }
}

/// System: carry out the commands editors sent.
pub fn control_command_system(
    channel: Option<ResMut<ControlChannel>>,
    timeline: Option<ResMut<Timeline>>,
    mut editor: Option<ResMut<EditorState>>,
    presentation: Option<Res<PresentationMode>>,
) {
    let (Some(mut channel), Some(mut timeline)) = (channel, timeline) else {
        return;
    };
    keep_requested_loop(&mut channel, &mut timeline, editor.as_deref_mut());
    let presenting = presentation.is_some_and(|mode| mode.active);
    let incoming: Vec<Incoming> = channel.commands.try_iter().collect();
    for command in incoming {
        let result = command.and_then(|command| match command {
            Command::Seek {
                time,
                segment,
                stop,
            } => seek_time(&timeline, time, segment.as_deref(), stop.as_deref()).map(Some),
            Command::Follow { file, line } => channel
                .index
                .as_ref()
                .and_then(|index| index.instant_at(&file, line))
                .map(Some)
                .ok_or_else(|| {
                    format!(
                        "follow: nothing in the timeline comes from {}:{line} or above it",
                        file.display()
                    )
                }),
            Command::Play => {
                timeline.is_playing = true;
                Ok(None)
            }
            Command::Pause => {
                timeline.is_playing = false;
                Ok(None)
            }
            Command::Loop {
                segment,
                start,
                end,
            } => {
                let request = loop_request(segment, start, end)?;
                let Some(editor) = editor.as_deref_mut().filter(|_| !presenting) else {
                    return Err("loop: a presentation does not loop".to_string());
                };
                let range = loop_range(&timeline, &request)?;
                editor.set_segment_loop(&mut timeline, range, true)?;
                channel.applied_loop = active_loop(&timeline, Some(editor));
                channel.loop_request = Some(request);
                Ok(None)
            }
            Command::Unloop => {
                if let Some(editor) = editor.as_deref_mut() {
                    editor.clear_segment_loop(&mut timeline);
                }
                channel.loop_request = None;
                channel.applied_loop = None;
                Ok(None)
            }
        });
        match result {
            Ok(Some(time)) => timeline.rest_at(time),
            Ok(None) => {}
            Err(message) => channel.send(json!({ "event": "error", "message": message })),
        }
    }
}

/// System: report where the preview rests when it moves.
pub fn playhead_event_system(
    channel: Option<ResMut<ControlChannel>>,
    timeline: Option<Res<Timeline>>,
    editor: Option<Res<EditorState>>,
) {
    let (Some(mut channel), Some(timeline)) = (channel, timeline) else {
        return;
    };
    if timeline.cached_duration <= 0.0 {
        return;
    }
    let looping = editor.is_some_and(|editor| editor.segment_loop_active());
    let playhead = Playhead::of(&timeline, looping);
    let Some(last) = channel.last_playhead.as_ref() else {
        channel.send(playhead.event());
        channel.last_playhead = Some(playhead);
        channel.last_sent = Some(Instant::now());
        return;
    };
    if *last == playhead {
        return;
    }
    // Moving within the same place while playing waits its turn; a seek, a
    // pause, a new segment or stop or a changed loop goes at once.
    let same_place = last.segment == playhead.segment
        && last.stop == playhead.stop
        && last.playing == playhead.playing
        && last.looped == playhead.looped;
    if same_place
        && playhead.playing
        && channel
            .last_sent
            .is_some_and(|sent| sent.elapsed() < PLAYHEAD_INTERVAL)
    {
        return;
    }
    channel.send(playhead.event());
    channel.last_playhead = Some(playhead);
    channel.last_sent = Some(Instant::now());
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaanim_timeline::timeline::{SegmentMetadata, SegmentStop};

    #[test]
    fn listen_addresses_stay_on_this_computer() {
        assert_eq!(ListenAddress::parse("stdio"), Ok(ListenAddress::Stdio));
        assert_eq!(
            ListenAddress::parse("localhost:7801"),
            Ok(ListenAddress::Tcp("127.0.0.1:7801".parse().unwrap()))
        );
        assert!(matches!(
            ListenAddress::parse("[::1]:7801"),
            Ok(ListenAddress::Tcp(_))
        ));
        assert!(ListenAddress::parse("0.0.0.0:7801").is_err());
        assert!(ListenAddress::parse("192.168.1.4:7801").is_err());
        assert!(ListenAddress::parse("pipe").is_err());
    }

    #[test]
    fn commands_are_json_lines() {
        assert_eq!(parse_line("  "), None);
        assert_eq!(
            parse_line(r#"{"cmd":"seek","segment":"Problema","stop":"dato"}"#),
            Some(Ok(Command::Seek {
                time: None,
                segment: Some("Problema".into()),
                stop: Some("dato".into())
            }))
        );
        assert_eq!(
            parse_line(r#"{"cmd":"follow","file":"main.py","line":12}"#),
            Some(Ok(Command::Follow {
                file: "main.py".into(),
                line: 12
            }))
        );
        assert_eq!(parse_line(r#"{"cmd":"pause"}"#), Some(Ok(Command::Pause)));
        for bad in [
            r#"{"cmd":"jump"}"#,
            "seek 3",
            r#"{"cmd":"seek","segment":"Portada","at":"end"}"#,
        ] {
            assert!(matches!(parse_line(bad), Some(Err(_))), "{bad}");
        }
    }

    fn timeline() -> Timeline {
        let segment = |id, name: &str, start, end, stops: &[(Option<&str>, f64)]| SegmentMetadata {
            id,
            name: name.into(),
            notes: None,
            target: None,
            start_time: start,
            end_time: end,
            stops: stops
                .iter()
                .map(|(name, time)| SegmentStop {
                    name: name.map(str::to_owned),
                    time: *time,
                    ambient: None,
                })
                .collect(),
        };
        let mut timeline = Timeline::default();
        timeline.cached_duration = 6.0;
        timeline.segments = vec![
            segment(1, "Portada", 0.0, 2.0, &[(Some("titulo"), 1.0)]),
            segment(2, "results · 1 · 1 · Datos", 2.0, 4.0, &[]),
            segment(
                3,
                "results · 1 · 2 · Curvas",
                4.0,
                6.0,
                &[(Some("dato"), 5.0)],
            ),
        ];
        timeline
    }

    #[test]
    fn seeks_resolve_times_segments_sections_and_stops() {
        let timeline = timeline();
        let seek = |time, segment, stop| seek_time(&timeline, time, segment, stop);
        assert_eq!(seek(Some(3.5), None, None), Ok(3.5));
        assert_eq!(seek(Some(60.0), None, None), Ok(6.0));
        assert!(seek(Some(-1.0), None, None).is_err());
        // A segment rests at its first stop, a Section at the first among
        // its steps.
        assert_eq!(seek(None, Some("portada"), None), Ok(1.0));
        assert_eq!(seek(None, Some("RESULTS"), None), Ok(5.0));
        assert_eq!(seek(None, Some("results"), Some("Dato")), Ok(5.0));
        assert_eq!(seek(None, None, Some("titulo")), Ok(1.0));
        assert!(seek(None, Some("Portada"), Some("dato")).is_err());
        assert!(seek(None, Some("Cierre"), None).is_err());
        assert!(seek(Some(1.0), Some("Portada"), None).is_err());
        assert!(seek(None, None, None).is_err());
    }

    #[test]
    fn the_playhead_names_its_segment_and_stop() {
        let mut timeline = timeline();
        timeline.current_time = 5.2;
        let playhead = Playhead::of(&timeline, false);
        assert_eq!(playhead.millis, 5200);
        assert_eq!(
            playhead.segment.as_deref(),
            Some("results · 1 · 2 · Curvas")
        );
        assert_eq!(playhead.stop.as_deref(), Some("dato"));
        assert_eq!(playhead.event()["time"], 5.2);
        assert!(playhead.event()["loop"].is_null());
        timeline.current_time = 0.5;
        assert_eq!(Playhead::of(&timeline, false).stop, None);
        // A loop is reported only while the preview's loop is on.
        timeline.loop_range = Some((2.0, 6.0));
        assert!(Playhead::of(&timeline, false).event()["loop"].is_null());
        assert_eq!(
            Playhead::of(&timeline, true).event()["loop"],
            json!([2.0, 6.0])
        );
    }

    /// A channel for tests, and the receiver of what it sends.
    fn channel() -> (ControlChannel, Sender<Incoming>, Receiver<String>) {
        let (command_tx, commands) = crossbeam_channel::unbounded();
        let (events, event_rx) = crossbeam_channel::unbounded();
        let channel = ControlChannel {
            commands,
            events,
            index: None,
            script: None,
            last_playhead: None,
            last_sent: None,
            loop_request: None,
            applied_loop: None,
            reloaded: false,
        };
        (channel, command_tx, event_rx)
    }

    #[test]
    fn loops_take_a_segment_a_section_or_a_range() {
        let timeline = timeline();
        let range = |segment: Option<&str>, start, end| {
            loop_request(segment.map(str::to_owned), start, end)
                .and_then(|request| loop_range(&timeline, &request))
        };
        assert_eq!(range(Some("portada"), None, None), Ok((0.0, 2.0)));
        // A Section spans its steps.
        assert_eq!(range(Some("RESULTS"), None, None), Ok((2.0, 6.0)));
        assert_eq!(range(None, Some(1.2), Some(3.4)), Ok((1.2, 3.4)));
        // The end stops at the end of the scene.
        assert_eq!(range(None, Some(5.0), Some(60.0)), Ok((5.0, 6.0)));
        for (segment, start, end) in [
            (Some("Cierre"), None, None),
            (None, Some(3.0), Some(3.0)),
            (None, Some(-1.0), Some(2.0)),
            (None, Some(6.0), Some(7.0)),
            (None, Some(f64::NAN), Some(2.0)),
            (None, Some(1.0), None),
            (Some("Portada"), Some(1.0), Some(2.0)),
        ] {
            assert!(
                range(segment, start, end).is_err(),
                "{segment:?} {start:?} {end:?}"
            );
        }
        assert_eq!(
            parse_line(r#"{"cmd":"loop","segment":"Problema"}"#),
            Some(Ok(Command::Loop {
                segment: Some("Problema".into()),
                start: None,
                end: None
            }))
        );
        assert_eq!(parse_line(r#"{"cmd":"unloop"}"#), Some(Ok(Command::Unloop)));
    }

    #[test]
    fn a_requested_loop_follows_reloads_until_the_preview_changes_it() {
        let (mut channel, _, events) = channel();
        let mut timeline = timeline();
        let mut editor = EditorState::default();
        let request = LoopRequest::Segment("results".into());
        let range = loop_range(&timeline, &request).unwrap();
        editor.set_segment_loop(&mut timeline, range, true).unwrap();
        channel.loop_request = Some(request);
        channel.applied_loop = active_loop(&timeline, Some(&editor));

        // A reload moves the section; its loop follows by name, without
        // moving the playhead.
        timeline.segments[2].end_time = 7.0;
        timeline.cached_duration = 7.0;
        timeline.seek_request = None;
        channel.reloaded = true;
        keep_requested_loop(&mut channel, &mut timeline, Some(&mut editor));
        assert_eq!(timeline.loop_range, Some((2.0, 7.0)));
        assert_eq!(channel.applied_loop, Some((2.0, 7.0)));
        assert_eq!(timeline.seek_request, None);

        // The preview's own loop control takes over: the request is dropped.
        editor
            .set_segment_loop(&mut timeline, (0.0, 2.0), false)
            .unwrap();
        keep_requested_loop(&mut channel, &mut timeline, Some(&mut editor));
        assert_eq!(channel.loop_request, None);

        // A section that disappears in a reload ends the loop with an error.
        channel.loop_request = Some(LoopRequest::Segment("Portada".into()));
        timeline.segments.remove(0);
        channel.reloaded = true;
        keep_requested_loop(&mut channel, &mut timeline, Some(&mut editor));
        assert!(!editor.segment_loop_active());
        assert_eq!(channel.loop_request, None);
        let sent: Value = serde_json::from_str(&events.try_recv().unwrap()).unwrap();
        assert_eq!(sent["event"], "error");
    }

    #[test]
    fn ready_carries_the_timeline_of_the_scene_shown() {
        let (command_tx, commands) = crossbeam_channel::unbounded();
        let (events, event_rx) = crossbeam_channel::unbounded();
        drop(command_tx);
        let mut channel = ControlChannel {
            commands,
            events,
            index: None,
            script: Some(PathBuf::from("/talk/main.py")),
            last_playhead: None,
            last_sent: None,
            loop_request: None,
            applied_loop: None,
            reloaded: false,
        };
        // A bundle runs no script: its timeline is null.
        assert!(channel.ready_event(2.0)["timeline"].is_null());

        let index = TimelineIndex {
            duration: 2.0,
            segments: vec![gaanim_api::canvas::IndexedSegment {
                name: "Portada".into(),
                section: None,
                start: 0.0,
                end: 2.0,
                location: Some(gaanim_core::console::ScriptLocation::new(
                    "/talk/main.py",
                    4,
                )),
                stops: Vec::new(),
            }],
            ..TimelineIndex::default()
        };
        channel.reloading(index);
        channel.ready(2.0);
        let sent: Vec<Value> = event_rx
            .try_iter()
            .map(|event| serde_json::from_str(&event).unwrap())
            .collect();
        assert_eq!(sent[0]["event"], "reload");
        let ready = &sent[1];
        assert_eq!(ready["event"], "ready");
        assert_eq!(ready["duration"], 2.0);
        assert_eq!(ready["timeline"]["schema"], crate::inspect::SCHEMA);
        assert_eq!(ready["timeline"]["segments"][0]["name"], "Portada");
        assert_eq!(ready["timeline"]["segments"][0]["line"], 4);
    }

    #[test]
    fn new_editors_hear_the_latest_ready_and_playhead() {
        let mut latest = Vec::new();
        for event in [
            r#"{"event":"ready","duration":6.0}"#,
            r#"{"event":"playhead","time":1.0}"#,
            r#"{"event":"error","message":"x"}"#,
            r#"{"event":"playhead","time":2.0}"#,
        ] {
            remember(&mut latest, event);
        }
        assert_eq!(
            latest,
            [
                r#"{"event":"ready","duration":6.0}"#,
                r#"{"event":"playhead","time":2.0}"#
            ]
        );
    }
}
