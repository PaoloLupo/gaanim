//! Audience polls and quizzes during a presentation.
//!
//! A scene authors its polls with `scene.poll` and `scene.quiz` and draws
//! them itself. While a presentation runs, this module opens on the relay
//! the poll whose window holds the playhead, closes it when the playhead
//! leaves, reveals a quiz when the playhead passes its `reveal()`, and
//! publishes what the relay reports (counts, a quiz's seconds left, the
//! leaderboard) in [`PollResults`], which the scene's poll values, bars and
//! live text read. A thread talks to the relay, so a frame never waits on the
//! network; outside a presentation the results are not live and the scene
//! shows its previews. Presenter View gets a panel to remove a player or
//! start the game over.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy_egui::{EguiContext, egui};
use gaanim_animation::polls::PollResults;
use gaanim_timeline::timeline::{PollSessionInfo, Timeline, TimelinePoll};
use serde::Deserialize;

use crate::PresentationMode;
use crate::presenter::PresenterCamera;

/// How often the relay's results are read while presenting.
const REFRESH: Duration = Duration::from_millis(1000);
/// How long a request to the relay may take.
const TIMEOUT: Duration = Duration::from_secs(5);
/// Players Presenter View lists.
const PANEL_PLAYERS: usize = 12;

pub(crate) struct AudiencePollsPlugin;

impl Plugin for AudiencePollsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudiencePolls>()
            .init_resource::<PollResults>()
            .add_systems(
                Update,
                audience_poll_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
            )
            .add_systems(
                crate::presenter::PresenterEguiPass,
                presenter_polls_panel_system.after(crate::presenter::presenter_view_system),
            );
    }
}

/// The relay session of the presentation, while one runs.
#[derive(Resource, Default)]
pub(crate) struct AudiencePolls {
    client: Option<PollClient>,
    /// The session a relay could not be used for, reported once.
    unusable: Option<PollSessionInfo>,
    /// Presenter View's reset button was pressed once and awaits a second
    /// press.
    confirm_reset: bool,
    /// The relay answer the results were last built from.
    seen: Option<u64>,
}

/// Keep the relay's open poll on the one the playhead is in, reveal quizzes
/// the playhead passed, and publish what the relay reports to the scene.
fn audience_poll_system(
    presentation: Res<PresentationMode>,
    timeline: Res<Timeline>,
    mut polls: ResMut<AudiencePolls>,
    mut results: ResMut<PollResults>,
) {
    let session = timeline
        .poll_session
        .as_ref()
        .filter(|_| presentation.active && !timeline.polls.is_empty());
    let Some(session) = session else {
        // Ending the presentation ends its session: the thread closes the
        // open question, and the scene goes back to its previews.
        polls.client = None;
        if *results != PollResults::default() {
            *results = PollResults::default();
        }
        return;
    };
    let current = polls.client.as_ref().map(|client| &client.session);
    if current != Some(session) && polls.unusable.as_ref() != Some(session) {
        polls.client = match PollClient::start(session.clone()) {
            Ok(client) => Some(client),
            Err(error) => {
                gaanim_core::console::warn("polls", error);
                polls.unusable = Some(session.clone());
                None
            }
        };
    }
    let Some(client) = &mut polls.client else {
        return;
    };
    let now = timeline.current_time;
    client.show(timeline.poll_open_at(now));
    for poll in &timeline.polls {
        if let Some(reveal) = poll.quiz.as_ref().and_then(|quiz| quiz.reveal)
            && now >= reveal - 1e-5
        {
            client.reveal(&poll.id);
        }
    }
    let seen = polls.seen;
    polls.seen = polls
        .client
        .as_ref()
        .and_then(|client| client.update(&mut results, seen));
}

// ---------------------------------------------------------------------------
// Relay client
// ---------------------------------------------------------------------------

enum Command {
    Open(TimelinePoll),
    Close,
    Reveal(String),
    Kick(String),
    Reset,
}

/// What the relay last reported, and when it arrived.
#[derive(Debug, Clone)]
struct Snapshot {
    /// Counts the relay's answers, so the scene rebuilds its results only
    /// from a new one.
    version: u64,
    counts: HashMap<Arc<str>, Vec<u32>>,
    /// Quiz deadlines, milliseconds on the relay's clock.
    deadlines: HashMap<Arc<str>, f64>,
    /// The relay's clock when it answered.
    relay_now: f64,
    received: Instant,
    leaderboard: Vec<(Arc<str>, u64)>,
    players: u32,
    connected: u32,
}

struct PollClient {
    session: PollSessionInfo,
    /// The poll the relay was last told to open.
    open: Option<String>,
    /// Quizzes the relay was told to reveal in this presentation.
    revealed: HashSet<String>,
    commands: Sender<Command>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
}

impl PollClient {
    fn start(session: PollSessionInfo) -> Result<Self, String> {
        let relay = session.relay.clone().ok_or_else(|| {
            "the scene's polls have no relay: set one with `gaanim relay use <URL>` and \
             reload, so its QR codes point to it"
                .to_string()
        })?;
        let key = gaanim_project::relay::key_for_code(&session.code)?;
        let api = RelayApi::new(&format!("{relay}/s/{}", session.code), &key)?;
        let snapshot = Arc::new(Mutex::new(None));
        let (commands, receiver) = mpsc::channel();
        let thread_snapshot = snapshot.clone();
        std::thread::Builder::new()
            .name("gaanim-polls".into())
            .spawn(move || run_relay_session(api, receiver, thread_snapshot))
            .map_err(|error| format!("could not start the poll client: {error}"))?;
        gaanim_core::console::info("polls", format!("votes go to {relay}/s/{}", session.code));
        Ok(Self {
            session,
            open: None,
            revealed: HashSet::new(),
            commands,
            snapshot,
        })
    }

    fn send(&self, command: Command) {
        // The thread only ends with the client, so the send cannot fail.
        let _ = self.commands.send(command);
    }

    fn show(&mut self, poll: Option<&TimelinePoll>) {
        let id = poll.map(|poll| poll.id.clone());
        if id == self.open {
            return;
        }
        self.open = id;
        self.send(match poll {
            Some(poll) => Command::Open(poll.clone()),
            None => Command::Close,
        });
    }

    fn reveal(&mut self, id: &str) {
        if self.revealed.insert(id.to_string()) {
            self.send(Command::Reveal(id.to_string()));
        }
    }

    fn snapshot(&self) -> Option<Snapshot> {
        self.snapshot.lock().ok()?.clone()
    }

    /// Bring the scene's results up to date. A new answer from the relay
    /// (once a second) rebuilds them; between answers only a quiz's clock
    /// moves. Returns the answer they now come from.
    fn update(&self, results: &mut ResMut<PollResults>, seen: Option<u64>) -> Option<u64> {
        let guard = self.snapshot.lock().ok()?;
        let Some(snapshot) = guard.as_ref() else {
            if !results.live {
                results.live = true;
            }
            return None;
        };
        if seen != Some(snapshot.version) {
            drop(guard);
            let latest = self.results();
            if **results != latest {
                **results = latest;
            }
            return self.snapshot().map(|snapshot| snapshot.version);
        }
        let elapsed = snapshot.received.elapsed().as_secs_f64();
        for (id, deadline) in &snapshot.deadlines {
            let left = ((deadline - snapshot.relay_now) / 1000.0 - elapsed).max(0.0);
            if results.remaining.get(id) != Some(&left) {
                results.remaining.insert(id.clone(), left);
            }
        }
        Some(snapshot.version)
    }

    /// What the scene shows now: live results, with each quiz's seconds
    /// left counted on from the relay's last answer.
    fn results(&self) -> PollResults {
        let Some(snapshot) = self.snapshot() else {
            return PollResults {
                live: true,
                ..Default::default()
            };
        };
        let elapsed = snapshot.received.elapsed().as_secs_f64();
        PollResults {
            live: true,
            counts: snapshot.counts,
            remaining: snapshot
                .deadlines
                .into_iter()
                .map(|(id, deadline)| {
                    let left = (deadline - snapshot.relay_now) / 1000.0 - elapsed;
                    (id, left.max(0.0))
                })
                .collect(),
            leaderboard: snapshot.leaderboard,
            players: snapshot.players,
        }
    }
}

/// Requests of one relay session.
struct RelayApi {
    agent: ureq::Agent,
    session_url: String,
    authorization: String,
}

#[derive(Deserialize)]
struct Results {
    #[serde(default)]
    polls: HashMap<String, PollCounts>,
    #[serde(default)]
    players: Vec<Player>,
    #[serde(default, rename = "playerCount")]
    player_count: u32,
    #[serde(default)]
    connected: u32,
    #[serde(default)]
    now: f64,
}

#[derive(Deserialize)]
struct PollCounts {
    #[serde(default)]
    counts: Vec<u32>,
    #[serde(default)]
    quiz: Option<QuizState>,
}

#[derive(Deserialize)]
struct QuizState {
    deadline: f64,
}

#[derive(Deserialize)]
struct Player {
    name: String,
    score: u64,
}

/// An HTTP agent with the system's TLS, which ureq is built with here.
fn https_agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new()
        .map_err(|error| format!("could not set up TLS for the poll relay: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout(TIMEOUT)
        .build())
}

impl RelayApi {
    fn new(session_url: &str, key: &str) -> Result<Self, String> {
        Ok(Self {
            agent: https_agent()?,
            session_url: session_url.to_string(),
            authorization: format!("Bearer {key}"),
        })
    }

    fn request(&self, method: &str, path: &str) -> ureq::Request {
        self.agent
            .request(method, &format!("{}/{path}", self.session_url))
            .set("Authorization", &self.authorization)
    }

    fn post(&self, path: &str, body: serde_json::Value) -> Result<(), String> {
        self.request("POST", path)
            .send_json(body)
            .map_err(describe)?;
        Ok(())
    }

    fn open(&self, poll: &TimelinePoll) -> Result<(), String> {
        let mut body = serde_json::json!({
            "id": poll.id,
            "question": poll.question,
            "options": poll.options,
        });
        if let Some(quiz) = &poll.quiz {
            body["correct"] = quiz.correct.into();
            body["time"] = quiz.time.into();
            body["points"] = quiz.points.into();
        }
        self.request("PUT", "poll")
            .send_json(body)
            .map_err(describe)?;
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.request("DELETE", "poll").call().map_err(describe)?;
        Ok(())
    }

    fn results(&self) -> Result<Snapshot, String> {
        let response = self.request("GET", "results").call().map_err(describe)?;
        let received = Instant::now();
        let results: Results = response.into_json().map_err(|error| error.to_string())?;
        let mut counts = HashMap::new();
        let mut deadlines = HashMap::new();
        for (id, poll) in results.polls {
            let id: Arc<str> = id.into();
            if let Some(quiz) = poll.quiz {
                deadlines.insert(id.clone(), quiz.deadline);
            }
            counts.insert(id, poll.counts);
        }
        Ok(Snapshot {
            version: 0,
            counts,
            deadlines,
            relay_now: results.now,
            received,
            leaderboard: results
                .players
                .into_iter()
                .map(|player| (Arc::from(player.name), player.score))
                .collect(),
            players: results.player_count,
            connected: results.connected,
        })
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

/// The relay thread: open and close polls, reveal quizzes, remove players
/// and reset the game as commanded, and read the results each [`REFRESH`],
/// retrying what failed on the next tick.
fn run_relay_session(
    api: RelayApi,
    commands: Receiver<Command>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
) {
    // What the relay should show, and whether it already does.
    let mut wanted: Option<TimelinePoll> = None;
    let mut synced = true;
    // One-off requests still to send, in order.
    let mut queued: Vec<Command> = Vec::new();
    let mut offline = false;
    let mut report = |result: &Result<(), String>| match result {
        Ok(()) if offline => {
            gaanim_core::console::info("polls", "the relay is reachable again");
            offline = false;
        }
        Err(error) if !offline => {
            gaanim_core::console::warn("polls", format!("relay unreachable, retrying: {error}"));
            offline = true;
        }
        _ => {}
    };
    loop {
        let mut next = match commands.recv_timeout(REFRESH) {
            Ok(command) => Some(command),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // Only the latest poll matters after a burst of steps; one-off
        // requests all go out.
        while let Some(command) = next.take().or_else(|| commands.try_recv().ok()) {
            match command {
                Command::Open(poll) => {
                    wanted = Some(poll);
                    synced = false;
                }
                Command::Close => {
                    wanted = None;
                    synced = false;
                }
                command => queued.push(command),
            }
        }
        if !synced {
            let result = match &wanted {
                Some(poll) => api.open(poll),
                None => api.close(),
            };
            report(&result);
            synced = result.is_ok();
            if !synced {
                continue;
            }
        }
        while let Some(command) = queued.first() {
            let result = match command {
                Command::Reveal(id) => api.post("reveal", serde_json::json!({ "id": id })),
                Command::Kick(name) => api.post("kick", serde_json::json!({ "name": name })),
                Command::Reset => api.post("reset", serde_json::json!({})),
                Command::Open(_) | Command::Close => Ok(()),
            };
            report(&result);
            match result {
                Ok(()) => {
                    let done = queued.remove(0);
                    // A reset forgets the open question: open it again.
                    if matches!(done, Command::Reset) && wanted.is_some() {
                        synced = false;
                    }
                }
                // A player already gone, or a quiz the relay does not know:
                // nothing to retry.
                Err(error) if error.contains("answered 404") => {
                    queued.remove(0);
                }
                Err(_) => break,
            }
        }
        let result = api.results().map(|mut latest| {
            if let Ok(mut snapshot) = snapshot.lock() {
                latest.version = snapshot.as_ref().map_or(0, |previous| previous.version + 1);
                *snapshot = Some(latest);
            }
        });
        report(&result);
    }
    // The presentation ended: stop taking votes.
    if wanted.is_some() {
        let _ = api.close();
    }
}

// ---------------------------------------------------------------------------
// Presenter View
// ---------------------------------------------------------------------------

/// A small window in Presenter View with the audience: phones connected,
/// players and their scores, a button to remove a player and one to start
/// the game over.
fn presenter_polls_panel_system(
    mut contexts: Query<&mut EguiContext, With<PresenterCamera>>,
    mut polls: ResMut<AudiencePolls>,
) {
    let Ok(mut context) = contexts.single_mut() else {
        return;
    };
    let Some(client) = &polls.client else {
        return;
    };
    let snapshot = client.snapshot();
    let code = client.session.code.clone();
    let mut kick = None;
    let mut reset = false;
    let confirm_reset = polls.confirm_reset;
    egui::Window::new(format!("Audience · {code}"))
        .id(egui::Id::new("gaanim-presenter-audience"))
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-12.0, -12.0))
        .default_width(260.0)
        .collapsible(true)
        .resizable(false)
        .show(context.get_mut(), |ui| {
            let Some(snapshot) = &snapshot else {
                ui.label("Connecting to the relay…");
                return;
            };
            ui.label(format!(
                "{} phones connected · {} players",
                snapshot.connected, snapshot.players
            ));
            if !snapshot.leaderboard.is_empty() {
                ui.separator();
                egui::Grid::new("gaanim-presenter-players")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        for (rank, (name, score)) in
                            snapshot.leaderboard.iter().take(PANEL_PLAYERS).enumerate()
                        {
                            ui.label(format!("{}", rank + 1));
                            ui.label(name.as_ref());
                            ui.label(format!("{score}"));
                            if ui
                                .small_button("Remove")
                                .on_hover_text("Remove this player and block the phone")
                                .clicked()
                            {
                                kick = Some(name.to_string());
                            }
                            ui.end_row();
                        }
                    });
            }
            ui.separator();
            let label = if confirm_reset {
                "Press again to erase every vote and player"
            } else {
                "Start the game over"
            };
            if ui.button(label).clicked() {
                reset = true;
            }
        });
    if let Some(name) = kick
        && let Some(client) = &polls.client
    {
        client.send(Command::Kick(name));
    }
    if reset {
        if confirm_reset {
            if let Some(client) = &mut polls.client {
                client.send(Command::Reset);
                // Quizzes revealed before the reset can be revealed again.
                client.revealed.clear();
            }
            polls.confirm_reset = false;
        } else {
            polls.confirm_reset = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn poll(id: &str, question: &str) -> TimelinePoll {
        TimelinePoll {
            id: id.into(),
            question: question.into(),
            options: vec!["Sí".into(), "No".into()],
            preview: vec![0, 0],
            segment: 0,
            open: 0.0,
            close: 1.0,
            quiz: None,
        }
    }

    /// Wait until `check` accepts the client's results.
    fn wait_for(client: &PollClient, check: impl Fn(&PollResults) -> bool) -> PollResults {
        for _ in 0..100 {
            let results = client.results();
            if client.snapshot().is_some() && check(&results) {
                return results;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the relay did not answer in time: {:?}", client.results());
    }

    /// Talks to a real relay: `npm run dev` in the relay repository, then
    /// `GAANIM_TEST_RELAY=http://localhost:8787 cargo test -p gaanim_editor
    /// polls -- --ignored`.
    #[test]
    #[ignore = "needs a running relay in GAANIM_TEST_RELAY"]
    fn votes_reach_the_presentation_through_a_relay() {
        let relay = std::env::var("GAANIM_TEST_RELAY").expect("set GAANIM_TEST_RELAY");
        let code = "TSTR2A";
        let session_url = format!("{relay}/s/{code}");
        let mut client = PollClient::start(PollSessionInfo {
            relay: Some(relay),
            code: code.into(),
        })
        .unwrap();
        let phone = https_agent().unwrap();
        let post = |path: &str, body: serde_json::Value| {
            phone.post(&format!("{session_url}/{path}")).send_json(body)
        };
        let vote = |poll: &str, voter: char, option: u32| {
            post(
                "vote",
                serde_json::json!({
                    "poll": poll, "option": option, "voter": voter.to_string().repeat(32),
                }),
            )
        };
        let current = || -> serde_json::Value {
            phone
                .get(&format!("{session_url}/poll"))
                .call()
                .unwrap()
                .into_json()
                .unwrap()
        };

        // Start from an empty session: earlier runs left polls and players.
        client.send(Command::Reset);
        wait_for(&client, |results| {
            results.counts.is_empty() && results.players == 0
        });
        let first = poll("p0-test", "¿Primera?");
        client.show(Some(&first));
        wait_for(&client, |results| results.counts.contains_key("p0-test"));
        assert_eq!(current()["question"], "¿Primera?");
        vote("p0-test", 'a', 0).unwrap();
        vote("p0-test", 'b', 1).unwrap();
        vote("p0-test", 'c', 1).unwrap();
        vote("p0-test", 'c', 0).unwrap();
        let results = wait_for(&client, |results| {
            results
                .counts
                .get("p0-test")
                .is_some_and(|votes| votes.iter().sum::<u32>() == 3)
        });
        assert_eq!(results.counts["p0-test"], [2, 1]);

        // A quiz: players join, answer against the clock, and the reveal
        // builds the leaderboard.
        let mut quiz = poll("q1-test", "¿2 + 2?");
        quiz.options = vec!["3".into(), "4".into()];
        quiz.quiz = Some(gaanim_timeline::timeline::TimelineQuiz {
            correct: 1,
            time: 20,
            points: 1000,
            reveal: None,
        });
        client.show(Some(&quiz));
        let results = wait_for(&client, |results| results.remaining.contains_key("q1-test"));
        let left = results.remaining["q1-test"];
        assert!(left > 15.0 && left <= 20.0, "seconds left: {left}");
        post(
            "join",
            serde_json::json!({ "voter": "a".repeat(32), "name": "Ana" }),
        )
        .unwrap();
        post(
            "join",
            serde_json::json!({ "voter": "b".repeat(32), "name": "Beto" }),
        )
        .unwrap();
        vote("q1-test", 'a', 1).unwrap();
        vote("q1-test", 'b', 0).unwrap();
        client.reveal("q1-test");
        let results = wait_for(&client, |results| results.players == 2);
        assert_eq!(results.leaderboard[0].0.as_ref(), "Ana");
        assert!(results.leaderboard[0].1 > 900);
        assert_eq!(results.leaderboard[1], (Arc::from("Beto"), 0));
        wait_for(&client, |_| vote("q1-test", 'a', 1).is_err());

        // Coming back to the first poll keeps its votes.
        client.show(Some(&first));
        let results = wait_for(&client, |_| current()["id"] == "p0-test");
        assert_eq!(results.counts["p0-test"], [2, 1]);

        client.send(Command::Kick("beto".into()));
        wait_for(&client, |results| results.players == 1);
        client.show(None);
        wait_for(&client, |_| current()["open"] == false);
    }
}
