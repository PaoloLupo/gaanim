//! Audience polls during a presentation.
//!
//! While a presentation rests on a stop authored with `scene.poll`, the
//! audience screen shows a QR code that opens the relay's voting page on a
//! phone, and the votes as they arrive. The relay is a Cloudflare Worker each
//! user deploys (`gaanim relay`); a thread talks to it, so a frame never waits
//! on the network. One session code serves the whole presentation: a phone
//! scans once and follows every question.

use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::egui::{self, Align2, Color32, FontId, Rect, Vec2, pos2, vec2};
use gaanim_timeline::timeline::{Timeline, TimelinePoll};
use qrcodegen::{QrCode, QrCodeEcc};
use serde::Deserialize;

use crate::PresentationMode;
use crate::export::ProjectPaths;
use crate::ui_kit::palette as kit;

/// How often an open poll's counts are read while it is shown.
const REFRESH: Duration = Duration::from_millis(1000);
/// How long a request to the relay may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// Session code characters: no 0/O or 1/I to confuse. 32 of them, so a
/// random byte maps to one without bias. The relay accepts the same set.
const CODE_ALPHABET: &[u8; 32] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;
/// Answer colors, the same as the voting page's buttons.
const ANSWER_COLORS: [Color32; 6] = [
    Color32::from_rgb(226, 71, 91),
    Color32::from_rgb(59, 125, 221),
    Color32::from_rgb(217, 162, 27),
    Color32::from_rgb(47, 158, 91),
    Color32::from_rgb(142, 91, 214),
    Color32::from_rgb(31, 158, 168),
];

pub(crate) struct AudiencePollsPlugin;

impl Plugin for AudiencePollsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudiencePolls>()
            .add_systems(Update, audience_poll_system)
            .add_systems(
                bevy_egui::EguiPrimaryContextPass,
                audience_poll_overlay_system,
            );
    }
}

/// The poll the audience sees and the relay session that collects its votes.
#[derive(Resource, Default)]
pub(crate) struct AudiencePolls {
    /// Started the first time a presentation shows a poll.
    client: Option<PollClient>,
    /// Whether a relay was looked for and none is set.
    no_relay: bool,
    shown: Option<TimelinePoll>,
}

/// Open the poll of the stop a presentation rests on, and close it when the
/// presentation moves on.
fn audience_poll_system(
    presentation: Res<PresentationMode>,
    timeline: Res<Timeline>,
    project: Option<Res<ProjectPaths>>,
    mut polls: ResMut<AudiencePolls>,
) {
    let wanted = presentation
        .active
        .then(|| timeline.poll_at(timeline.current_time))
        .flatten();
    if wanted == polls.shown.as_ref() {
        return;
    }
    let wanted = wanted.cloned();
    if wanted.is_some() {
        let project_relay = project
            .as_ref()
            .and_then(|paths| paths.poll_relay.as_deref());
        let relay = gaanim_project::relay::resolve(project_relay).map(|(url, _)| url);
        polls.no_relay = relay.is_none();
        let current = polls.client.as_ref().map(|client| client.relay.as_str());
        if relay.as_deref() != current {
            polls.client = relay.and_then(|relay| match PollClient::start(relay) {
                Ok(client) => Some(client),
                Err(error) => {
                    gaanim_core::console::warn("polls", error);
                    None
                }
            });
        }
    }
    if let Some(client) = &mut polls.client {
        client.show(wanted.clone());
    }
    polls.shown = wanted;
}

// ---------------------------------------------------------------------------
// Relay client
// ---------------------------------------------------------------------------

/// A presentation's session on the relay: its code, which phones use, and
/// the key that lets only this presentation open questions and read votes.
#[derive(Debug, Clone)]
struct Session {
    code: String,
    key: String,
}

impl Session {
    fn random() -> Result<Self, String> {
        let mut bytes = [0u8; CODE_LENGTH + 32];
        getrandom::fill(&mut bytes)
            .map_err(|error| format!("could not create a poll session: {error}"))?;
        let (code, key) = bytes.split_at(CODE_LENGTH);
        Ok(Self {
            code: code
                .iter()
                .map(|byte| char::from(CODE_ALPHABET[usize::from(*byte) % CODE_ALPHABET.len()]))
                .collect(),
            key: key.iter().map(|byte| format!("{byte:02x}")).collect(),
        })
    }
}

enum Command {
    Open { generation: u64, poll: TimelinePoll },
    Close,
}

#[derive(Debug, Clone, PartialEq)]
enum Status {
    Connecting,
    Live,
    Offline(String),
}

/// What the relay last reported for the open poll.
#[derive(Debug, Clone)]
struct LiveResults {
    /// The [`Command::Open`] these results belong to.
    generation: u64,
    status: Status,
    counts: Vec<u32>,
    total: u32,
}

struct PollClient {
    relay: String,
    code: String,
    join_url: String,
    qr: Option<QrCode>,
    generation: u64,
    commands: Sender<Command>,
    live: Arc<Mutex<LiveResults>>,
}

impl PollClient {
    fn start(relay: String) -> Result<Self, String> {
        let session = Session::random()?;
        let join_url = format!("{relay}/s/{}", session.code);
        let live = Arc::new(Mutex::new(LiveResults {
            generation: 0,
            status: Status::Connecting,
            counts: Vec::new(),
            total: 0,
        }));
        let (commands, receiver) = mpsc::channel();
        let api = RelayApi::new(&join_url, &session.key)?;
        let thread_live = live.clone();
        std::thread::Builder::new()
            .name("gaanim-polls".into())
            .spawn(move || run_relay_session(api, receiver, thread_live))
            .map_err(|error| format!("could not start the poll client: {error}"))?;
        Ok(Self {
            qr: QrCode::encode_text(&join_url, QrCodeEcc::Medium).ok(),
            relay,
            code: session.code,
            join_url,
            generation: 0,
            commands,
            live,
        })
    }

    fn show(&mut self, poll: Option<TimelinePoll>) {
        let command = match poll {
            Some(poll) => {
                self.generation += 1;
                Command::Open {
                    generation: self.generation,
                    poll,
                }
            }
            None => Command::Close,
        };
        // The thread only ends with the client, so the send cannot fail.
        let _ = self.commands.send(command);
    }

    /// The results of the poll shown last, once the relay has them.
    fn results(&self) -> Option<LiveResults> {
        let live = self.live.lock().ok()?.clone();
        (live.generation == self.generation).then_some(live)
    }
}

/// Requests of one relay session.
struct RelayApi {
    agent: ureq::Agent,
    session_url: String,
    authorization: String,
}

#[derive(Deserialize)]
struct Opened {
    id: String,
}

#[derive(Deserialize)]
struct Counts {
    id: Option<String>,
    #[serde(default)]
    counts: Vec<u32>,
    #[serde(default)]
    total: u32,
}

impl RelayApi {
    fn new(session_url: &str, key: &str) -> Result<Self, String> {
        let tls = native_tls::TlsConnector::new()
            .map_err(|error| format!("could not set up TLS for the poll relay: {error}"))?;
        Ok(Self {
            agent: ureq::AgentBuilder::new()
                .tls_connector(Arc::new(tls))
                .timeout(TIMEOUT)
                .build(),
            session_url: session_url.to_string(),
            authorization: format!("Bearer {key}"),
        })
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        self.agent
            .request(method, &format!("{}/{path}", self.session_url))
            .set("Authorization", &self.authorization)
    }

    fn open(&self, poll: &TimelinePoll) -> Result<String, String> {
        let body = serde_json::json!({ "question": poll.question, "options": poll.options });
        let response = self
            .request("PUT", "poll")
            .send_json(body)
            .map_err(describe)?;
        let opened: Opened = response.into_json().map_err(|error| error.to_string())?;
        Ok(opened.id)
    }

    fn close(&self) -> Result<(), String> {
        self.request("DELETE", "poll").call().map_err(describe)?;
        Ok(())
    }

    fn counts(&self) -> Result<Counts, String> {
        let response = self.request("GET", "results").call().map_err(describe)?;
        response.into_json().map_err(|error| error.to_string())
    }
}

fn describe(error: ureq::Error) -> String {
    match error {
        ureq::Error::Status(code, response) => {
            let reason = response
                .into_json::<serde_json::Value>()
                .ok()
                .and_then(|body| body["error"].as_str().map(str::to_owned));
            match reason {
                Some(reason) => format!("the relay answered {code}: {reason}"),
                None => format!("the relay answered {code}"),
            }
        }
        ureq::Error::Transport(transport) => transport.to_string(),
    }
}

/// The relay thread: open and close questions as commanded and read the
/// counts of the open one every [`REFRESH`], retrying after failures.
fn run_relay_session(api: RelayApi, commands: Receiver<Command>, live: Arc<Mutex<LiveResults>>) {
    let publish = |update: &dyn Fn(&mut LiveResults)| {
        if let Ok(mut live) = live.lock() {
            update(&mut live);
        }
    };
    let mut wanted: Option<TimelinePoll> = None;
    let mut opened: Option<String> = None;
    let mut close = false;
    loop {
        let wait = if wanted.is_some() || close {
            REFRESH
        } else {
            Duration::from_secs(3600)
        };
        let mut next = match commands.recv_timeout(wait) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // Only the latest command matters after a burst of steps.
        while let Some(command) = next.take().or_else(|| commands.try_recv().ok()) {
            match command {
                Command::Open { generation, poll } => {
                    publish(&|live| {
                        *live = LiveResults {
                            generation,
                            status: Status::Connecting,
                            counts: vec![0; poll.options.len()],
                            total: 0,
                        }
                    });
                    wanted = Some(poll);
                    close = false;
                }
                Command::Close => {
                    close = wanted.take().is_some() || close;
                }
            }
            opened = None;
        }
        let offline = |error: String| publish(&|live| live.status = Status::Offline(error.clone()));
        if close {
            match api.close() {
                Ok(()) => close = false,
                Err(error) => offline(error),
            }
        }
        let Some(poll) = &wanted else {
            continue;
        };
        if opened.is_none() {
            match api.open(poll) {
                Ok(id) => opened = Some(id),
                Err(error) => {
                    offline(error);
                    continue;
                }
            }
        }
        match api.counts() {
            Ok(counts) if counts.id == opened => publish(&|live| {
                live.status = Status::Live;
                live.counts = counts.counts.clone();
                live.total = counts.total;
            }),
            // The relay no longer has this question (it expired): ask again.
            Ok(_) => opened = None,
            Err(error) => offline(error),
        }
    }
    // The presentation closed: stop taking votes for its last question.
    if opened.is_some() {
        let _ = api.close();
    }
}

// ---------------------------------------------------------------------------
// Audience screen
// ---------------------------------------------------------------------------

/// Paint the poll over the audience screen. It is painted on a layer, not
/// an area, so a click still advances the presentation.
fn audience_poll_overlay_system(
    mut contexts: bevy_egui::EguiContexts,
    presentation: Res<PresentationMode>,
    polls: Res<AudiencePolls>,
) {
    if !presentation.active {
        return;
    }
    let Some(poll) = &polls.shown else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Middle,
        egui::Id::new("gaanim-audience-poll"),
    ));
    let screen = ctx.viewport_rect();
    let results = polls.client.as_ref().and_then(PollClient::results);
    let status = match (&polls.client, &results) {
        (None, _) if polls.no_relay => {
            "No relay is set: run `gaanim relay init` and `gaanim relay use <URL>`".to_string()
        }
        (None, _) => "Audience polls are unavailable; see the console".to_string(),
        (Some(_), None)
        | (
            Some(_),
            Some(LiveResults {
                status: Status::Connecting,
                ..
            }),
        ) => "Connecting to the relay…".to_string(),
        (Some(_), Some(results)) => match &results.status {
            Status::Offline(error) => format!("Relay unreachable, retrying: {error}"),
            _ => match results.total {
                1 => "1 vote".to_string(),
                total => format!("{total} votes"),
            },
        },
    };
    paint_poll(
        &painter,
        screen,
        poll,
        polls.client.as_ref(),
        results.as_ref(),
        &status,
    );
}

fn paint_poll(
    painter: &egui::Painter,
    screen: Rect,
    poll: &TimelinePoll,
    client: Option<&PollClient>,
    results: Option<&LiveResults>,
    status: &str,
) {
    painter.rect_filled(screen, 0.0, Color32::from_black_alpha(220));
    let unit = (screen.height() / 54.0).max(8.0);
    let card = screen.shrink2(vec2(screen.width() * 0.05, screen.height() * 0.08));

    // Left: the QR code and the address it opens.
    let mut answers_left = card.left();
    if let Some(client) = client {
        let side = (card.height() * 0.66).min(card.width() * 0.34);
        let qr_rect = Rect::from_min_size(card.left_top(), Vec2::splat(side));
        if let Some(qr) = &client.qr {
            paint_qr(painter, qr_rect, qr);
        }
        let mut y = qr_rect.bottom() + unit * 1.2;
        let address = client
            .join_url
            .trim_start_matches("https://")
            .trim_start_matches("http://");
        let galley = painter.layout(
            address.to_string(),
            FontId::proportional(unit * 1.1),
            kit::TEXT_MUTED,
            side,
        );
        let height = galley.size().y;
        painter.galley(pos2(qr_rect.left(), y), galley, kit::TEXT_MUTED);
        y += height + unit * 0.6;
        painter.text(
            pos2(qr_rect.left(), y),
            Align2::LEFT_TOP,
            &client.code,
            FontId::monospace(unit * 3.0),
            kit::TEXT,
        );
        answers_left = qr_rect.right() + unit * 3.0;
    }

    // Right: the question and a bar per answer.
    let width = card.right() - answers_left;
    let question = painter.layout(
        poll.question.clone(),
        FontId::proportional(unit * 2.4),
        kit::TEXT,
        width,
    );
    let mut y = card.top();
    let question_height = question.size().y;
    painter.galley(pos2(answers_left, y), question, kit::TEXT);
    y += question_height + unit * 2.0;

    let footer = unit * 2.0;
    let count = poll.options.len().max(1) as f32;
    let row = ((card.bottom() - footer - y) / count).clamp(unit * 2.0, unit * 4.5);
    let bar_height = row * 0.78;
    let counts = results
        .map(|results| results.counts.as_slice())
        .unwrap_or(&[]);
    let total = results.map_or(0, |results| results.total);
    let most = counts.iter().copied().max().unwrap_or(0).max(1);
    for (index, option) in poll.options.iter().enumerate() {
        let votes = counts.get(index).copied().unwrap_or(0);
        let color = ANSWER_COLORS[index % ANSWER_COLORS.len()];
        let bar = Rect::from_min_size(pos2(answers_left, y), vec2(width, bar_height));
        painter.rect_filled(bar, 0.0, kit::SURFACE);
        let filled = bar.width() * votes as f32 / most as f32;
        painter.rect_filled(
            Rect::from_min_size(bar.min, vec2(filled, bar.height())),
            0.0,
            color.gamma_multiply(0.85),
        );
        painter.rect_filled(
            Rect::from_min_size(bar.min, vec2(unit * 0.4, bar.height())),
            0.0,
            color,
        );
        let label_size = (bar_height * 0.42).min(unit * 1.6);
        painter.text(
            pos2(bar.left() + unit * 1.2, bar.center().y),
            Align2::LEFT_CENTER,
            format!("{}   {option}", char::from(b'A' + index as u8)),
            FontId::proportional(label_size),
            kit::TEXT,
        );
        let share = if total == 0 {
            String::new()
        } else {
            format!("  ·  {}%", (votes * 100 + total / 2) / total)
        };
        painter.text(
            pos2(bar.right() - unit * 1.2, bar.center().y),
            Align2::RIGHT_CENTER,
            format!("{votes}{share}"),
            FontId::proportional(label_size),
            kit::TEXT,
        );
        y += row;
    }
    painter.text(
        pos2(card.right(), card.bottom()),
        Align2::RIGHT_BOTTOM,
        status,
        FontId::proportional(unit * 1.1),
        kit::TEXT_MUTED,
    );
}

/// Paint `qr` black on white inside `rect`, with a quiet zone, merging each
/// row's dark runs so adjacent modules show no seams.
fn paint_qr(painter: &egui::Painter, rect: Rect, qr: &QrCode) {
    const QUIET: i32 = 3;
    let size = qr.size();
    let cell = rect.width() / (size + 2 * QUIET) as f32;
    painter.rect_filled(rect, 0.0, Color32::WHITE);
    for (y, runs) in (0..size).map(|y| (y, dark_runs(size, |x| qr.get_module(x, y)))) {
        for (start, end) in runs {
            let min = rect.min + vec2((start + QUIET) as f32, (y + QUIET) as f32) * cell;
            let run = Rect::from_min_size(min, vec2((end - start) as f32, 1.0) * cell);
            painter.rect_filled(run.expand(cell * 0.02), 0.0, Color32::BLACK);
        }
    }
}

/// `[start, end)` of each run of dark modules in a row of `size`.
fn dark_runs(size: i32, dark: impl Fn(i32) -> bool) -> Vec<(i32, i32)> {
    let mut runs = Vec::new();
    let mut start = None;
    for x in 0..=size {
        match (start, x < size && dark(x)) {
            (None, true) => start = Some(x),
            (Some(from), false) => {
                runs.push((from, x));
                start = None;
            }
            _ => {}
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sessions_use_unambiguous_codes_and_long_keys() {
        let session = Session::random().unwrap();
        assert_eq!(session.code.len(), CODE_LENGTH);
        assert!(
            session
                .code
                .bytes()
                .all(|byte| CODE_ALPHABET.contains(&byte))
        );
        assert!(!session.code.contains(['0', 'O', '1', 'I']));
        assert_eq!(session.key.len(), 64);
        assert!(session.key.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(session.key, Session::random().unwrap().key);
    }

    #[test]
    fn a_join_address_fits_a_qr_code() {
        let url = "https://gaanim-relay.someone-with-a-long-name.workers.dev/s/ABCDEF";
        let qr = QrCode::encode_text(url, QrCodeEcc::Medium).unwrap();
        assert!(qr.size() <= 45, "version too high to scan across a room");
    }

    #[test]
    fn dark_runs_merge_adjacent_modules() {
        let row = [true, true, false, true, false, false, true];
        assert_eq!(
            dark_runs(row.len() as i32, |x| row[x as usize]),
            [(0, 2), (3, 4), (6, 7)]
        );
        assert!(dark_runs(3, |_| false).is_empty());
    }

    fn poll(question: &str) -> TimelinePoll {
        TimelinePoll {
            time: 1.0,
            question: question.into(),
            options: vec!["Sí".into(), "No".into()],
        }
    }

    /// Wait until `check` accepts the client's results.
    fn wait_for(client: &PollClient, check: impl Fn(&LiveResults) -> bool) -> LiveResults {
        for _ in 0..100 {
            if let Some(results) = client.results().filter(|results| check(results)) {
                return results;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the relay did not answer in time: {:?}", client.results());
    }

    /// Talks to a real relay: `npx wrangler dev` in the `gaanim relay init`
    /// template, then `GAANIM_TEST_RELAY=http://localhost:8787 cargo test
    /// -p gaanim_editor polls -- --ignored`.
    #[test]
    #[ignore = "needs a running relay in GAANIM_TEST_RELAY"]
    fn votes_reach_the_presentation_through_a_relay() {
        let relay = std::env::var("GAANIM_TEST_RELAY").expect("set GAANIM_TEST_RELAY");
        let mut client = PollClient::start(relay).unwrap();
        let join = client.join_url.clone();
        let phone = ureq::agent();
        let vote = |poll_id: &str, voter: &str, option: u32| {
            phone
                .post(&format!("{join}/vote"))
                .send_json(serde_json::json!({ "poll": poll_id, "option": option, "voter": voter }))
        };
        let current = || -> serde_json::Value {
            phone
                .get(&format!("{join}/poll"))
                .call()
                .unwrap()
                .into_json()
                .unwrap()
        };

        client.show(Some(poll("¿Primera?")));
        wait_for(&client, |results| results.status == Status::Live);
        let first = current();
        assert_eq!(first["question"], "¿Primera?");
        let id = first["id"].as_str().unwrap().to_string();
        let voters = ["a".repeat(32), "b".repeat(32), "c".repeat(32)];
        vote(&id, &voters[0], 0).unwrap();
        vote(&id, &voters[1], 1).unwrap();
        vote(&id, &voters[2], 1).unwrap();
        // A phone may change its vote; it still counts once.
        vote(&id, &voters[2], 0).unwrap();
        let results = wait_for(&client, |results| results.total == 3);
        assert_eq!(results.counts, [2, 1]);
        assert!(vote(&id, &voters[0], 5).is_err());

        // The next question starts from zero and the old one takes no votes.
        client.show(Some(poll("¿Segunda?")));
        wait_for(&client, |results| {
            results.status == Status::Live && current()["question"] == "¿Segunda?"
        });
        assert!(vote(&id, &voters[0], 0).is_err());
        assert_eq!(client.results().unwrap().total, 0);

        client.show(None);
        for _ in 0..50 {
            if current()["open"] == false {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the relay kept the question open");
    }
}
