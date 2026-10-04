//! Audience polls and quizzes during a presentation.
//!
//! A scene authors its polls with `scene.poll` and `scene.quiz` and draws
//! them itself. While a presentation runs, this module opens on the relay
//! the poll whose window holds the playhead, closes it when the playhead
//! leaves, reveals a quiz when the playhead passes its `reveal()`, and
//! publishes what the relay reports (counts, a quiz's seconds left, the
//! leaderboard, the audience in joining order) in [`PollResults`], which the scene's poll values, bars and
//! live text read. A thread talks to the relay, so a frame never waits on the
//! network; outside a presentation the results are not live and the scene
//! shows its previews. The relay pushes the results over a WebSocket as they
//! change; while it cannot, the thread asks for them every [`REFRESH`].
//! A stop authored with `until` advances by itself once the audience meets
//! its condition, when the presentation came to it going forward.
//! Presenter View shows the audience and can remove a
//! player or start a new game; so can `R` twice in a row, and
//! `gaanim relay reset` from a terminal. A presentation that starts again
//! keeps the game, so a crash in the middle of a talk loses nothing.
//!
//! The web player has no threads: there the browser makes the requests and
//! holds the socket, and [`web::RelaySession`] does each frame what the
//! native thread does in its loop. Of its two pages, only the audience's
//! talks to the relay; Presenter View sees the audience and asks for
//! changes through [`crate::presenter_link`].

use std::collections::{HashMap, HashSet};
#[cfg(not(target_arch = "wasm32"))]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::platform::time::Instant;
use bevy::prelude::*;
use gaanim_animation::polls::{AUDIENCE_AGE_CAP, PollResults};
use gaanim_timeline::timeline::{PollSessionInfo, Timeline, TimelinePoll};
use serde::{Deserialize, Serialize};

use crate::PresentationMode;
use crate::presenter_link::{AudienceRequest, LinkRole, PresenterLink};

#[cfg(target_arch = "wasm32")]
pub mod web;

/// How often the relay's results are read while presenting, when its
/// socket does not push them.
const REFRESH: Duration = Duration::from_millis(1000);
/// How long a request to the relay may take.
#[cfg(not(target_arch = "wasm32"))]
const TIMEOUT: Duration = Duration::from_secs(5);
/// How long a first press of "new game" waits for the second one.
const CONFIRM_RESET: Duration = Duration::from_secs(5);
/// Least time between two writes of the saved results while votes arrive.
#[cfg(not(target_arch = "wasm32"))]
const SAVE_EVERY: Duration = Duration::from_secs(5);
/// The folder, inside a project's output folder or beside a bundle, where
/// presentations keep their games' results.
pub const RESULTS_FOLDER: &str = "resultados";

pub(crate) struct AudiencePollsPlugin;

impl Plugin for AudiencePollsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudiencePolls>()
            .init_resource::<PollResults>()
            .add_systems(
                Update,
                (
                    audience_poll_system,
                    stop_gate_system,
                    share_audience_system,
                )
                    .chain()
                    .in_set(gaanim_scene::hierarchy::SceneSet::Input),
            );
    }
}

/// The relay session of the presentation, while one runs.
#[derive(Resource, Default)]
pub(crate) struct AudiencePolls {
    client: Option<PollClient>,
    /// The session a relay could not be used for, reported once.
    unusable: Option<PollSessionInfo>,
    /// When "new game" was pressed once; a second press within
    /// [`CONFIRM_RESET`] erases the game.
    confirm_reset: Option<Instant>,
    /// The relay answer the results were last built from.
    seen: Option<u64>,
    /// The playhead on the previous frame, to tell arriving at a stop going
    /// forward from going back to it.
    previous_time: Option<f64>,
    /// The stop the presentation rests on, and whether its gate may
    /// advance it.
    resting: Option<(f64, bool)>,
    /// Where the gated stop the presentation rests on stands, for the
    /// speaker.
    gate: Option<String>,
    /// What the web player's Presenter View page was last told.
    shared: Option<Option<AudienceView>>,
    /// Whether votes were live and which relay answer Presenter View was
    /// last passed (audience page).
    shared_results: Option<(bool, Option<u64>)>,
    /// The game and answers Presenter View already has (audience page).
    shared_answers: (Option<String>, Answers),
    /// Presenter View: whether the audience page collects votes, and the
    /// relay answer it passed on.
    followed: bool,
    following: Mutex<Option<Snapshot>>,
}

/// Advance a gated stop once its condition holds. Only a stop reached going
/// forward advances by itself, so going back to one whose condition still
/// holds does not throw the presentation forward again; the stop a
/// presentation starts on counts as reached going forward.
fn stop_gate_system(
    presentation: Res<PresentationMode>,
    results: Res<PollResults>,
    mut timeline: ResMut<Timeline>,
    mut polls: ResMut<AudiencePolls>,
    link: Option<Res<PresenterLink>>,
) {
    // The web player's audience page advances and Presenter View follows:
    // both advancing could take the presentation two steps.
    let follower = link.is_some_and(|link| link.role() == LinkRole::Presenter);
    if !presentation.active || follower {
        polls.previous_time = None;
        polls.resting = None;
        polls.gate = None;
        return;
    }
    let now = timeline.current_time;
    let previous = polls.previous_time.replace(now);
    let resting = timeline.resting_stop().filter(|_| results.live);
    let Some(stop) = resting else {
        polls.resting = None;
        polls.gate = None;
        return;
    };
    let Some(until) = timeline.stop_gate_at(stop).map(|gate| gate.until.clone()) else {
        polls.resting = None;
        polls.gate = None;
        return;
    };
    if polls.resting.map(|(time, _)| time) != Some(stop) {
        let forward = previous.is_none_or(|previous| previous < stop - 1e-5);
        polls.resting = Some((stop, forward));
    }
    let armed = polls.resting.is_some_and(|(_, armed)| armed);
    polls.gate = Some(if armed {
        format!("Advances by itself: {}", until.progress(&results))
    } else {
        format!("Would advance by itself: {}", until.progress(&results))
    });
    if armed && until.holds(&results) {
        polls.resting = Some((stop, false));
        timeline.advance();
    }
}

/// Keep the relay's open poll on the one the playhead is in, reveal quizzes
/// the playhead passed, and publish what the relay reports to the scene.
fn audience_poll_system(
    presentation: Res<PresentationMode>,
    timeline: Res<Timeline>,
    mut polls: ResMut<AudiencePolls>,
    mut results: ResMut<PollResults>,
    project: Option<Res<crate::export::ProjectPaths>>,
    bundle: Option<Res<crate::bundle_player::BundlePlayback>>,
    link: Option<ResMut<PresenterLink>>,
) {
    // The web player's Presenter View page leaves the relay to the
    // audience page, and shows the results it passes on.
    if let Some(mut link) = link.filter(|link| link.role() == LinkRole::Presenter) {
        polls.client = None;
        if let Some((live, snapshot)) = link.take_results() {
            polls.followed = live;
            match snapshot {
                // Polls left out kept their answers, as with the relay.
                Some(shared) => store(
                    &polls.following,
                    Snapshot::from_shared(shared, Instant::now()),
                ),
                None => {
                    if let Ok(mut following) = polls.following.lock() {
                        *following = None;
                    }
                }
            }
            polls.seen = None;
        }
        if !polls.followed {
            if *results != PollResults::default() {
                *results = PollResults::default();
            }
            return;
        }
        let seen = polls.seen;
        polls.seen = update_results(&polls.following, &mut results, seen);
        return;
    }
    let session = timeline
        .poll_session
        .as_ref()
        .filter(|session| presentation.active && (!timeline.polls.is_empty() || session.lobby));
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
        // A bundle keeps its results beside it; a script, in its project's
        // output folder.
        let folder = match (&bundle, &project) {
            (Some(bundle), _) => bundle
                .path()
                .parent()
                .map(|parent| parent.join(RESULTS_FOLDER)),
            (None, Some(project)) => Some(project.output_dir.join(RESULTS_FOLDER)),
            (None, None) => None,
        };
        polls.client = match PollClient::start(session.clone(), folder) {
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
    client.stage(timeline.game_stage(now));
    // After a new game, the quizzes already behind the playhead stay
    // unrevealed: revealing them would show phones old questions.
    let quiet = std::mem::take(&mut client.after_reset);
    for poll in &timeline.polls {
        if let Some(reveal) = poll.quiz.as_ref().and_then(|quiz| quiz.reveal)
            && passed(reveal, now)
        {
            if quiet {
                client.revealed.insert(poll.id.clone());
            } else {
                client.reveal(&poll.id);
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    client.web.pump();
    let seen = polls.seen;
    polls.seen = polls
        .client
        .as_ref()
        .and_then(|client| client.update(&mut results, seen));
}

/// The web player's two pages: the audience page tells Presenter View who
/// is playing, and does what Presenter View asks of the audience.
fn share_audience_system(
    link: Option<ResMut<PresenterLink>>,
    mut polls: ResMut<AudiencePolls>,
    mut timeline: ResMut<Timeline>,
) {
    let Some(mut link) = link else {
        return;
    };
    if link.role() != LinkRole::Audience {
        return;
    }
    for request in link.take_audience_requests() {
        match request {
            AudienceRequest::Kick { name } => polls.kick(&name),
            AudienceRequest::Reset => {
                if polls.press_reset() {
                    crate::presenter::restart_game(&mut timeline);
                }
            }
            AudienceRequest::CancelReset => polls.cancel_reset(),
            AudienceRequest::SaveResults => polls.save_results(),
        }
    }
    let hello = link.take_peer_hello();
    let view = polls.view();
    if hello || polls.shared.as_ref() != Some(&view) {
        link.send_audience(view.clone());
        polls.shared = Some(view);
    }
    // Each relay answer once, as it arrives, not every frame.
    let live = polls.client.is_some();
    let version = polls.client.as_ref().and_then(PollClient::version);
    if hello || polls.shared_results != Some((live, version)) {
        let snapshot = polls.client.as_ref().and_then(PollClient::snapshot);
        let shared = snapshot.map(|snapshot| polls.share(&snapshot, hello));
        link.send_results(live, shared);
        polls.shared_results = Some((live, version));
    }
}

/// Whether the playhead at `now` went past a quiz's `reveal` time. Resting
/// on it is not enough: a `reveal()` written right after a stop shares the
/// stop's time, and must wait until the presentation advances from it.
fn passed(reveal: f64, now: f64) -> bool {
    now > reveal + gaanim_api::canvas::REVEAL_AFTER
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
    /// Phones ask for a nickname as soon as they open the page.
    Lobby,
    /// The game plays in these teams.
    Teams(gaanim_timeline::timeline::TeamsInfo),
    /// Joining also asks this (`scene.roster`).
    Ask(gaanim_timeline::timeline::AskInfo),
    /// Save the game's results now, as Presenter View asks.
    SaveResults,
    /// Where the game is: "play", "podium", or "end" when the
    /// presentation ends.
    Stage(&'static str),
}

/// Each player's answer, by poll id and nickname.
type Answers = HashMap<Arc<str>, HashMap<Arc<str>, gaanim_animation::polls::PlayerAnswer>>;

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
    /// Players in joining order, with when they joined on the relay's clock.
    audience: Vec<(Arc<str>, f64)>,
    /// The character each player made, by nickname.
    avatars: HashMap<Arc<str>, gaanim_animation::characters::CharacterParts>,
    players: u32,
    connected: u32,
    teams: Vec<gaanim_animation::polls::TeamResult>,
    player_teams: HashMap<Arc<str>, usize>,
    stats: HashMap<Arc<str>, gaanim_animation::polls::PlayerStats>,
    /// Every answer heard in this game, by poll and nickname: the relay
    /// sends all of them on connecting and then the open poll's.
    answers: Answers,
    respondents: HashMap<Arc<str>, u32>,
    latest: Option<Arc<str>>,
    /// The game the relay is in: a new one forgets earlier answers.
    game: Option<String>,
}

impl Snapshot {
    /// The relay's `results`, received at `received`.
    fn from_results(results: Results, received: Instant) -> Self {
        let mut counts = HashMap::new();
        let mut deadlines = HashMap::new();
        let catalog = gaanim_animation::characters::character_catalog();
        let avatars = results
            .audience
            .iter()
            .filter_map(|arrival| Some((arrival.name.as_str(), arrival.avatar?)))
            .chain(
                results
                    .players
                    .iter()
                    .filter_map(|player| Some((player.name.as_str(), player.avatar?))),
            )
            .filter(|(_, character)| catalog.contains(character))
            .map(|(name, character)| (Arc::from(name), character))
            .collect();
        let player_teams = results
            .audience
            .iter()
            .filter_map(|arrival| Some((arrival.name.as_str(), arrival.team?)))
            .chain(
                results
                    .players
                    .iter()
                    .filter_map(|player| Some((player.name.as_str(), player.team?))),
            )
            .map(|(name, team)| (Arc::from(name), team))
            .collect();
        let teams = results
            .teams
            .iter()
            .map(|team| gaanim_animation::polls::TeamResult {
                score: team.score,
                players: team.players,
            })
            .collect();
        let stats = results
            .audience
            .iter()
            .filter_map(|arrival| Some((arrival.name.as_str(), arrival.stats?)))
            .chain(
                results
                    .players
                    .iter()
                    .filter_map(|player| Some((player.name.as_str(), player.stats?))),
            )
            .map(|(name, stats)| (Arc::from(name), stats.into()))
            .collect();
        let mut answers = HashMap::new();
        let mut respondents = HashMap::new();
        for (id, poll) in results.polls {
            let id: Arc<str> = id.into();
            if let Some(quiz) = poll.quiz {
                deadlines.insert(id.clone(), quiz.deadline);
            }
            if let Some(count) = poll.respondents {
                respondents.insert(id.clone(), count);
            }
            if let Some(list) = poll.answers {
                answers.insert(
                    id.clone(),
                    list.into_iter()
                        .map(|answer| (Arc::from(answer.name.as_str()), answer.into()))
                        .collect(),
                );
            }
            counts.insert(id, poll.counts);
        }
        Self {
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
            audience: results
                .audience
                .into_iter()
                .map(|arrival| (Arc::from(arrival.name), arrival.joined))
                .collect(),
            avatars,
            players: results.player_count,
            connected: results.connected,
            teams,
            player_teams,
            stats,
            answers,
            respondents,
            latest: results.latest.map(|latest| Arc::from(latest.as_str())),
            game: results.game,
        }
    }

    /// Seconds since each player joined, `elapsed` seconds after the relay
    /// answered.
    fn audience(&self, elapsed: f64) -> Vec<(Arc<str>, f64)> {
        self.audience
            .iter()
            .map(|(name, joined)| {
                let age = (self.relay_now - joined) / 1000.0 + elapsed;
                (name.clone(), age.clamp(0.0, AUDIENCE_AGE_CAP))
            })
            .collect()
    }

    /// This answer as the audience page sends it to Presenter View, with the
    /// relay's clock moved on to now and the answers of the polls `send`
    /// accepts.
    fn share(&self, send: impl Fn(&Arc<str>) -> bool) -> SharedSnapshot {
        SharedSnapshot {
            version: self.version,
            counts: strings(&self.counts),
            deadlines: strings(&self.deadlines),
            relay_now: self.relay_now + self.received.elapsed().as_secs_f64() * 1000.0,
            leaderboard: string_pairs(&self.leaderboard),
            audience: string_pairs(&self.audience),
            avatars: strings(&self.avatars),
            players: self.players,
            connected: self.connected,
            teams: self.teams.clone(),
            player_teams: strings(&self.player_teams),
            stats: strings(&self.stats),
            answers: self
                .answers
                .iter()
                .filter(|(poll, _)| send(poll))
                .map(|(poll, answers)| (poll.to_string(), strings(answers)))
                .collect(),
            respondents: strings(&self.respondents),
            latest: self.latest.as_deref().map(str::to_string),
            game: self.game.clone(),
        }
    }

    /// What the audience page sent, as received at `received`.
    fn from_shared(shared: SharedSnapshot, received: Instant) -> Self {
        Self {
            version: shared.version,
            counts: arcs(shared.counts),
            deadlines: arcs(shared.deadlines),
            relay_now: shared.relay_now,
            received,
            leaderboard: arc_pairs(shared.leaderboard),
            audience: arc_pairs(shared.audience),
            avatars: arcs(shared.avatars),
            players: shared.players,
            connected: shared.connected,
            teams: shared.teams,
            player_teams: arcs(shared.player_teams),
            stats: arcs(shared.stats),
            answers: shared
                .answers
                .into_iter()
                .map(|(poll, answers)| (Arc::from(poll), arcs(answers)))
                .collect(),
            respondents: arcs(shared.respondents),
            latest: shared.latest.map(Arc::from),
            game: shared.game,
        }
    }
}

/// The relay's last answer as the web player's audience page forwards it to
/// its Presenter View, which has no relay of its own. Names travel as
/// strings (serde's `rc` feature is off).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct SharedSnapshot {
    version: u64,
    counts: HashMap<String, Vec<u32>>,
    deadlines: HashMap<String, f64>,
    /// The relay's clock when the audience page sent it.
    relay_now: f64,
    leaderboard: Vec<(String, u64)>,
    audience: Vec<(String, f64)>,
    avatars: HashMap<String, gaanim_animation::characters::CharacterParts>,
    players: u32,
    connected: u32,
    teams: Vec<gaanim_animation::polls::TeamResult>,
    player_teams: HashMap<String, usize>,
    stats: HashMap<String, gaanim_animation::polls::PlayerStats>,
    answers: HashMap<String, HashMap<String, gaanim_animation::polls::PlayerAnswer>>,
    respondents: HashMap<String, u32>,
    latest: Option<String>,
    game: Option<String>,
}

fn strings<V: Clone>(map: &HashMap<Arc<str>, V>) -> HashMap<String, V> {
    map.iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn arcs<V>(map: HashMap<String, V>) -> HashMap<Arc<str>, V> {
    map.into_iter()
        .map(|(key, value)| (Arc::from(key), value))
        .collect()
}

fn string_pairs<V: Clone>(pairs: &[(Arc<str>, V)]) -> Vec<(String, V)> {
    pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

fn arc_pairs<V>(pairs: Vec<(String, V)>) -> Vec<(Arc<str>, V)> {
    pairs
        .into_iter()
        .map(|(key, value)| (Arc::from(key), value))
        .collect()
}

pub(crate) struct PollClient {
    session: PollSessionInfo,
    /// The poll the relay was last told to open.
    open: Option<String>,
    /// Quizzes the relay was told to reveal in this presentation.
    revealed: HashSet<String>,
    /// A new game was just started.
    after_reset: bool,
    /// The game stage the relay was last told.
    stage: Option<gaanim_timeline::timeline::GameStage>,
    commands: Sender<Command>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
    /// The results folder, when this presentation keeps its games.
    #[cfg(not(target_arch = "wasm32"))]
    results: Option<std::path::PathBuf>,
    /// What the thread last saved there.
    #[cfg(not(target_arch = "wasm32"))]
    saved: Arc<Mutex<SavedResults>>,
    /// Receives the results the relay pushes; ends with the client.
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "held for its Drop, which closes it")
    )]
    socket: ResultsSocket,
    /// The browser's requests and socket, driven each frame.
    #[cfg(target_arch = "wasm32")]
    web: web::RelaySession,
}

impl PollClient {
    /// Start the session's client; a game's results are kept in
    /// `results`, when given.
    #[cfg(not(target_arch = "wasm32"))]
    fn start(
        session: PollSessionInfo,
        results: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        let relay = session.relay.clone().ok_or_else(|| {
            "the scene's polls have no relay: set one with `gaanim relay use <URL>` and \
             reload, so its QR codes point to it"
                .to_string()
        })?;
        let key = gaanim_project::relay::key_for_code(&session.code)?;
        let api = RelayApi::new(&format!("{relay}/s/{}", session.code), &key)?;
        let snapshot = Arc::new(Mutex::new(None));
        let socket = ResultsSocket::start(&api.session_url, &api.authorization, snapshot.clone())?;
        let (commands, receiver) = mpsc::channel();
        if session.lobby {
            let _ = commands.send(Command::Lobby);
        }
        if let Some(teams) = &session.teams {
            let _ = commands.send(Command::Teams(teams.clone()));
        }
        if let Some(ask) = &session.ask {
            let _ = commands.send(Command::Ask(ask.clone()));
        }
        let thread_snapshot = snapshot.clone();
        let pushed = socket.live.clone();
        let checked = relay.clone();
        let saved = Arc::new(Mutex::new(SavedResults::default()));
        let keeper = results
            .clone()
            .map(|folder| ResultsKeeper::new(folder, saved.clone()));
        std::thread::Builder::new()
            .name("gaanim-polls".into())
            .spawn(move || {
                // An outdated relay fails in odd ways: say so once, up front.
                match relay_version(&checked) {
                    Ok(version) => {
                        if let Some(advice) = gaanim_project::relay::version_advice(version) {
                            gaanim_core::console::warn("polls", advice);
                        }
                    }
                    Err(error) => gaanim_core::console::warn("polls", error),
                }
                run_relay_session(api, receiver, thread_snapshot, pushed, keeper)
            })
            .map_err(|error| format!("could not start the poll client: {error}"))?;
        gaanim_core::console::info("polls", format!("votes go to {relay}/s/{}", session.code));
        Ok(Self {
            session,
            open: None,
            revealed: HashSet::new(),
            after_reset: false,
            stage: None,
            commands,
            snapshot,
            results,
            saved,
            socket,
        })
    }

    /// Start the session's client in the web player, which offers a game's
    /// results as a download instead of keeping them.
    #[cfg(target_arch = "wasm32")]
    fn start(
        session: PollSessionInfo,
        _results: Option<std::path::PathBuf>,
    ) -> Result<Self, String> {
        let relay = session.relay.clone().ok_or_else(|| {
            "the presentation's polls have no relay: record it again after \
             `gaanim relay use <URL>`"
                .to_string()
        })?;
        let snapshot = Arc::new(Mutex::new(None));
        let (commands, receiver) = mpsc::channel();
        if session.lobby {
            let _ = commands.send(Command::Lobby);
        }
        if let Some(teams) = &session.teams {
            let _ = commands.send(Command::Teams(teams.clone()));
        }
        if let Some(ask) = &session.ask {
            let _ = commands.send(Command::Ask(ask.clone()));
        }
        let web = web::RelaySession::start(&relay, &session.code, receiver, snapshot.clone())?;
        gaanim_core::console::info("polls", format!("votes go to {relay}/s/{}", session.code));
        Ok(Self {
            session,
            open: None,
            revealed: HashSet::new(),
            after_reset: false,
            stage: None,
            commands,
            snapshot,
            web,
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

    fn stage(&mut self, stage: gaanim_timeline::timeline::GameStage) {
        if self.stage != Some(stage) {
            self.stage = Some(stage);
            self.send(Command::Stage(stage.name()));
        }
    }

    fn reveal(&mut self, id: &str) {
        if self.revealed.insert(id.to_string()) {
            self.send(Command::Reveal(id.to_string()));
        }
    }

    fn snapshot(&self) -> Option<Snapshot> {
        self.snapshot.lock().ok()?.clone()
    }

    /// The relay answer the client holds, without copying it.
    fn version(&self) -> Option<u64> {
        self.snapshot
            .lock()
            .ok()?
            .as_ref()
            .map(|snapshot| snapshot.version)
    }

    fn update(&self, results: &mut ResMut<PollResults>, seen: Option<u64>) -> Option<u64> {
        update_results(&self.snapshot, results, seen)
    }

    /// What the scene shows now.
    #[cfg(test)]
    fn results(&self) -> PollResults {
        self.snapshot().map_or_else(
            || PollResults {
                live: true,
                ..Default::default()
            },
            results_of,
        )
    }
}

/// Bring the scene's results up to date with `snapshot`. A new answer from
/// the relay (once a second) rebuilds them; between answers only a quiz's
/// clock moves. Returns the answer they now come from.
fn update_results(
    snapshot: &Mutex<Option<Snapshot>>,
    results: &mut ResMut<PollResults>,
    seen: Option<u64>,
) -> Option<u64> {
    let guard = snapshot.lock().ok()?;
    let Some(snapshot) = guard.as_ref() else {
        if !results.live {
            results.live = true;
        }
        return None;
    };
    if seen != Some(snapshot.version) {
        let latest = results_of(snapshot.clone());
        if **results != latest {
            **results = latest;
        }
        return Some(snapshot.version);
    }
    let elapsed = snapshot.received.elapsed().as_secs_f64();
    for (id, deadline) in &snapshot.deadlines {
        let left = ((deadline - snapshot.relay_now) / 1000.0 - elapsed).max(0.0);
        if results.remaining.get(id) != Some(&left) {
            results.remaining.insert(id.clone(), left);
        }
    }
    // Players who joined in the last minute are still arriving.
    if results
        .audience
        .iter()
        .any(|(_, age)| *age < AUDIENCE_AGE_CAP)
    {
        results.audience = snapshot.audience(elapsed);
    }
    Some(snapshot.version)
}

/// What the scene shows from `snapshot`: live results, with each quiz's
/// seconds left counted on from the relay's answer.
fn results_of(snapshot: Snapshot) -> PollResults {
    let elapsed = snapshot.received.elapsed().as_secs_f64();
    PollResults {
        live: true,
        audience: snapshot.audience(elapsed),
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
        connected: snapshot.connected,
        avatars: snapshot.avatars,
        teams: snapshot.teams,
        player_teams: snapshot.player_teams,
        stats: snapshot.stats,
        answers: snapshot.answers,
        respondents: snapshot.respondents,
        latest: snapshot.latest,
    }
}

#[cfg(not(target_arch = "wasm32"))]
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
    /// Players in joining order.
    #[serde(default)]
    audience: Vec<Arrival>,
    #[serde(default)]
    connected: u32,
    #[serde(default)]
    now: f64,
    /// Each team's standing, in the scene's order.
    #[serde(default)]
    teams: Vec<TeamState>,
    /// The poll open now, or the last one opened.
    #[serde(default)]
    latest: Option<String>,
    #[serde(default)]
    game: Option<String>,
}

/// A player's game so far, as the relay reports it.
#[derive(Deserialize, Clone, Copy)]
struct StatsState {
    #[serde(default)]
    score: u64,
    #[serde(default)]
    correct: u32,
    #[serde(default)]
    answered: u32,
    #[serde(default)]
    streak: u32,
}

impl From<StatsState> for gaanim_animation::polls::PlayerStats {
    fn from(stats: StatsState) -> Self {
        Self {
            score: stats.score,
            correct: stats.correct,
            answered: stats.answered,
            streak: stats.streak,
        }
    }
}

/// One player's answer on a poll.
#[derive(Deserialize)]
struct AnswerState {
    name: String,
    #[serde(default)]
    options: Vec<usize>,
    #[serde(default)]
    elapsed: f64,
    #[serde(default)]
    points: u32,
    #[serde(default)]
    right: Option<bool>,
}

impl From<AnswerState> for gaanim_animation::polls::PlayerAnswer {
    fn from(answer: AnswerState) -> Self {
        Self {
            options: answer
                .options
                .iter()
                .filter(|option| **option < 32)
                .fold(0, |options, option| options | 1 << option),
            elapsed: answer.elapsed / 1000.0,
            points: answer.points,
            right: answer.right,
        }
    }
}

#[derive(Deserialize)]
struct TeamState {
    #[serde(default)]
    score: u64,
    #[serde(default)]
    players: u32,
}

#[derive(Deserialize)]
struct PollCounts {
    #[serde(default)]
    counts: Vec<u32>,
    #[serde(default)]
    quiz: Option<QuizState>,
    /// Phones that answered; a multiple choice poll's votes add up to more.
    #[serde(default)]
    respondents: Option<u32>,
    /// Each player's answer: all polls' on connecting, then the open one's.
    #[serde(default)]
    answers: Option<Vec<AnswerState>>,
}

#[derive(Deserialize)]
struct QuizState {
    deadline: f64,
}

#[derive(Deserialize)]
struct Player {
    name: String,
    score: u64,
    #[serde(default)]
    avatar: Option<gaanim_animation::characters::CharacterParts>,
    #[serde(default)]
    team: Option<usize>,
    #[serde(default, flatten)]
    stats: Option<StatsState>,
}

#[derive(Deserialize)]
struct Arrival {
    name: String,
    /// When the player joined, milliseconds on the relay's clock.
    #[serde(default)]
    joined: f64,
    #[serde(default)]
    avatar: Option<gaanim_animation::characters::CharacterParts>,
    #[serde(default)]
    team: Option<usize>,
    #[serde(default, flatten)]
    stats: Option<StatsState>,
}

#[cfg(not(target_arch = "wasm32"))]
/// An HTTP agent with the system's TLS, which ureq is built with here.
fn https_agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new()
        .map_err(|error| format!("could not set up TLS for the poll relay: {error}"))?;
    Ok(ureq::AgentBuilder::new()
        .tls_connector(Arc::new(tls))
        .timeout(TIMEOUT)
        .build())
}

#[cfg(not(target_arch = "wasm32"))]
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
        self.request("PUT", "poll")
            .send_json(open_body(poll))
            .map_err(describe)?;
        Ok(())
    }

    /// Store a poll's picture on the relay, for phones to load.
    fn upload(&self, image: &gaanim_timeline::timeline::PollImage) -> Result<(), String> {
        self.request("PUT", &format!("image/{}", image.hash))
            .set("Content-Type", &image.mime)
            .send_bytes(&image.bytes)
            .map_err(describe)?;
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.request("DELETE", "poll").call().map_err(describe)?;
        Ok(())
    }

    /// The whole game, for keeping its results.
    fn report(&self) -> Result<crate::poll_report::Report, String> {
        self.request("GET", "report")
            .call()
            .map_err(describe)?
            .into_json()
            .map_err(|error| format!("the relay's report is not valid: {error}"))
    }

    fn results(&self) -> Result<Snapshot, String> {
        let response = self.request("GET", "results").call().map_err(describe)?;
        let received = Instant::now();
        let results: Results = response.into_json().map_err(|error| error.to_string())?;
        Ok(Snapshot::from_results(results, received))
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// The protocol version the relay at `relay` reports on `/health`.
pub fn relay_version(relay: &str) -> Result<u64, String> {
    let health: serde_json::Value = https_agent()?
        .get(&format!("{relay}/health"))
        .call()
        .map_err(|error| format!("could not reach the relay at {relay}: {}", describe(error)))?
        .into_json()
        .map_err(|_| format!("{relay} is not a Gaanim relay"))?;
    match (health["relay"].as_str(), health["version"].as_u64()) {
        (Some("gaanim"), Some(version)) => Ok(version),
        _ => Err(format!("{relay} is not a Gaanim relay")),
    }
}

#[cfg(not(target_arch = "wasm32"))]
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

#[cfg(not(target_arch = "wasm32"))]
/// The relay thread: open and close polls, reveal quizzes, remove players
/// and reset the game as commanded, retrying what failed on the next tick,
/// and read the results each [`REFRESH`] while the socket does not push
/// them (`pushed`).
fn run_relay_session(
    api: RelayApi,
    commands: Receiver<Command>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
    pushed: Arc<AtomicBool>,
    mut keeper: Option<ResultsKeeper>,
) {
    // Presenter View asked to save the results now.
    let mut save_now = false;
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
    // Pictures the relay already has.
    let mut uploaded: HashSet<String> = HashSet::new();
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
                Command::SaveResults => save_now = true,
                // Only the latest stage matters.
                Command::Stage(stage) => {
                    queued.retain(|queued| !matches!(queued, Command::Stage(_)));
                    queued.push(Command::Stage(stage));
                }
                command => queued.push(command),
            }
        }
        if !synced {
            let result = match &wanted {
                // A picture goes up once, before its question.
                Some(poll) => match &poll.image {
                    Some(image) if !uploaded.contains(&image.hash) => api
                        .upload(image)
                        .inspect(|()| {
                            uploaded.insert(image.hash.clone());
                        })
                        .and_then(|()| api.open(poll)),
                    _ => api.open(poll),
                },
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
                Command::Lobby => api.post("lobby", serde_json::json!({ "open": true })),
                Command::Teams(teams) => api.post(
                    "teams",
                    serde_json::json!({
                        "names": teams.names,
                        "colors": teams.colors,
                        "choose": teams.choose,
                    }),
                ),
                Command::Stage(stage) => api.post("stage", serde_json::json!({ "stage": stage })),
                Command::Ask(ask) => api.post(
                    "ask",
                    serde_json::json!({ "label": ask.label, "required": ask.required }),
                ),
                Command::Open(_) | Command::Close | Command::SaveResults => Ok(()),
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
        if !pushed.load(Ordering::Relaxed) {
            let result = api.results().map(|latest| store(&snapshot, latest));
            report(&result);
        }
        if let Some(keeper) = &mut keeper {
            let version = snapshot
                .lock()
                .ok()
                .and_then(|latest| latest.as_ref().map(|s| s.version));
            keeper.keep(&api, version, std::mem::take(&mut save_now));
        }
    }
    // The presentation ended: keep the game's final results, stop taking
    // votes, and phones say goodbye.
    if let Some(keeper) = &mut keeper {
        keeper.keep(&api, None, true);
    }
    if wanted.is_some() {
        let _ = api.close();
    }
    let _ = api.post("stage", serde_json::json!({ "stage": "end" }));
}

#[cfg(not(target_arch = "wasm32"))]
/// What the results keeper last did, for Presenter View.
#[derive(Debug, Clone, Default)]
struct SavedResults {
    /// The game's folder, once written.
    folder: Option<std::path::PathBuf>,
    at: Option<Instant>,
    error: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
/// Keeps a presentation's games in the results folder: after the results
/// change, at most every [`SAVE_EVERY`], and once more when it ends, so a
/// presentation that closes unexpectedly still leaves them.
struct ResultsKeeper {
    folder: std::path::PathBuf,
    status: Arc<Mutex<SavedResults>>,
    /// The results version written last.
    seen: Option<u64>,
    written: Option<Instant>,
    /// Games already announced in the terminal.
    announced: HashSet<String>,
    /// The last failure, reported once.
    failed: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ResultsKeeper {
    fn new(folder: std::path::PathBuf, status: Arc<Mutex<SavedResults>>) -> Self {
        Self {
            folder,
            status,
            seen: None,
            written: None,
            announced: HashSet::new(),
            failed: None,
        }
    }

    /// Write the game when `version` is new and enough time passed, or
    /// right away with `now`.
    fn keep(&mut self, api: &RelayApi, version: Option<u64>, now: bool) {
        if !now {
            let changed = version.is_some() && version != self.seen;
            let rested = self
                .written
                .is_none_or(|written| written.elapsed() >= SAVE_EVERY);
            if !(changed && rested) {
                return;
            }
        }
        let result = api.report().and_then(|report| {
            if report.is_empty() {
                return Ok(None);
            }
            report
                .write(&self.folder)
                .map(|folder| Some((report.game, folder)))
        });
        self.seen = version.or(self.seen);
        self.written = Some(Instant::now());
        if let Ok(mut status) = self.status.lock() {
            match &result {
                Ok(Some((_, folder))) => {
                    status.folder = Some(folder.clone());
                    status.at = Some(Instant::now());
                    status.error = None;
                }
                Ok(None) => {}
                Err(error) => status.error = Some(error.clone()),
            }
        }
        match result {
            Ok(Some((game, folder))) => {
                self.failed = None;
                if self.announced.insert(game.unwrap_or_default()) {
                    gaanim_core::console::info(
                        "polls",
                        format!("the game's results go to {}", folder.display()),
                    );
                }
            }
            Ok(None) => {}
            Err(error) => {
                if self.failed.as_ref() != Some(&error) {
                    gaanim_core::console::warn(
                        "polls",
                        format!("could not save the results: {error}"),
                    );
                }
                self.failed = Some(error);
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Download the game the relay holds for the project (or script) at `path`
/// and write its results into `output`, or the project's results folder.
/// Returns the game's folder.
pub fn save_relay_results(
    path: &std::path::Path,
    output: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, String> {
    let directory = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(
                || std::path::PathBuf::from("."),
                std::path::Path::to_path_buf,
            )
    };
    let directory = directory.canonicalize().unwrap_or(directory);
    let (scope, project_relay) = gaanim_project::relay::scope_of(&directory);
    let (relay, _) = gaanim_project::relay::resolve(project_relay.as_deref())
        .ok_or_else(|| "no relay is set: run `gaanim relay use <URL>`".to_string())?;
    let session = gaanim_project::relay::session_for(&scope)?;
    let report = RelayApi::new(&format!("{relay}/s/{}", session.code), &session.key)?
        .report()
        .map_err(|error| {
            if error.contains("unknown session") {
                format!(
                    "the relay at {relay} holds no game for session {}: present the project first",
                    session.code
                )
            } else {
                error
            }
        })?;
    if report.is_empty() {
        return Err(format!(
            "the relay holds no answers for session {} (it forgets a game 12 hours after its last activity)",
            session.code
        ));
    }
    let folder = match output {
        Some(output) => output.to_path_buf(),
        None => results_folder_of(&scope),
    };
    report.write(&folder)
}

#[cfg(not(target_arch = "wasm32"))]
/// Where the project (or script folder) `scope` keeps its results: the
/// results folder in its output folder.
fn results_folder_of(scope: &std::path::Path) -> std::path::PathBuf {
    let output = gaanim_project::resolve_project(scope)
        .ok()
        .map(|project| {
            if project.manifest.output_dir.is_absolute() {
                project.manifest.output_dir.clone()
            } else {
                project.root.join(&project.manifest.output_dir)
            }
        })
        .unwrap_or_else(|| scope.join("exports"));
    output.join(RESULTS_FOLDER)
}

// ---------------------------------------------------------------------------
// Results socket
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
/// Keepalive on the results socket, answered by the relay without waking
/// the session.
const PING: Duration = Duration::from_secs(25);
#[cfg(not(target_arch = "wasm32"))]
/// How long the socket may stay silent, keepalives included, before it
/// counts as lost.
const SILENCE: Duration = Duration::from_secs(60);
#[cfg(not(target_arch = "wasm32"))]
/// How often a blocked read wakes to send keepalives and notice the end.
const READ_TICK: Duration = Duration::from_millis(500);
#[cfg(not(target_arch = "wasm32"))]
/// Longest wait between attempts to reconnect.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

#[cfg(not(target_arch = "wasm32"))]
/// The presentation's WebSocket on the relay, which pushes the results each
/// time they change. While it is down, the relay thread asks over HTTP.
struct ResultsSocket {
    /// Whether results arrive on the socket now.
    live: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

#[cfg(not(target_arch = "wasm32"))]
impl ResultsSocket {
    fn start(
        session_url: &str,
        authorization: &str,
        snapshot: Arc<Mutex<Option<Snapshot>>>,
    ) -> Result<Self, String> {
        let url = format!(
            "{}/presenter",
            session_url
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        );
        let live = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let authorization = authorization.to_string();
        let (thread_live, thread_stop) = (live.clone(), stop.clone());
        std::thread::Builder::new()
            .name("gaanim-polls-socket".into())
            .spawn(move || {
                run_results_socket(&url, &authorization, &snapshot, &thread_live, &thread_stop);
            })
            .map_err(|error| format!("could not start the poll client: {error}"))?;
        Ok(Self { live, stop })
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for ResultsSocket {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Why a results socket could not be used.
enum SocketError {
    /// A relay without the results socket (older than API 6): polling it
    /// is all there is.
    Unsupported,
    Failed(String),
}

#[cfg(not(target_arch = "wasm32"))]
type RelaySocket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<TcpStream>>;

#[cfg(not(target_arch = "wasm32"))]
/// Open the results socket: TCP with a timeout, TLS for `wss`, and the
/// handshake with the presenter's key.
fn connect_results(url: &str, authorization: &str) -> Result<RelaySocket, SocketError> {
    use tungstenite::client::IntoClientRequest;
    let failed = |error: &dyn std::fmt::Display| SocketError::Failed(error.to_string());
    let mut request = url.into_client_request().map_err(|error| failed(&error))?;
    let header =
        tungstenite::http::HeaderValue::from_str(authorization).map_err(|error| failed(&error))?;
    request
        .headers_mut()
        .insert(tungstenite::http::header::AUTHORIZATION, header);
    let host = request
        .uri()
        .host()
        .ok_or_else(|| failed(&"the relay address has no host"))?
        .to_string();
    let secure = request.uri().scheme_str() == Some("wss");
    let port = request
        .uri()
        .port_u16()
        .unwrap_or(if secure { 443 } else { 80 });
    let mut last = None;
    let mut stream = None;
    for address in (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| failed(&error))?
    {
        match TcpStream::connect_timeout(&address, TIMEOUT) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = Some(error),
        }
    }
    let stream = stream.ok_or_else(|| match last {
        Some(error) => failed(&error),
        None => failed(&"the relay address resolves to nothing"),
    })?;
    stream
        .set_read_timeout(Some(TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(TIMEOUT)))
        .map_err(|error| failed(&error))?;
    let connector = if secure {
        let tls = native_tls::TlsConnector::new().map_err(|error| failed(&error))?;
        tungstenite::Connector::NativeTls(tls)
    } else {
        tungstenite::Connector::Plain
    };
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, None, Some(connector))
        .map_err(|error| match error {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response))
            if matches!(response.status().as_u16(), 404 | 426) =>
        {
            SocketError::Unsupported
        }
        tungstenite::HandshakeError::Failure(error) => failed(&error),
        tungstenite::HandshakeError::Interrupted(_) => failed(&"the handshake timed out"),
    })?;
    // Reads wake every READ_TICK from now on, to keep the socket alive and
    // to notice the presentation ending.
    let tcp = match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(tcp) => Some(tcp),
        tungstenite::stream::MaybeTlsStream::NativeTls(tls) => Some(tls.get_ref()),
        _ => None,
    };
    if let Some(tcp) = tcp {
        tcp.set_read_timeout(Some(READ_TICK))
            .map_err(|error| failed(&error))?;
    }
    Ok(socket)
}

#[cfg(not(target_arch = "wasm32"))]
/// Keep the results socket open until `stop`, reconnecting with backoff,
/// and put each result it pushes into `snapshot`.
fn run_results_socket(
    url: &str,
    authorization: &str,
    snapshot: &Mutex<Option<Snapshot>>,
    live: &AtomicBool,
    stop: &AtomicBool,
) {
    let mut backoff = Duration::from_secs(1);
    // Whether the last failure was reported, so a streak shows once.
    let mut reported = false;
    while !stop.load(Ordering::Relaxed) {
        match connect_results(url, authorization) {
            Ok(mut socket) => {
                backoff = Duration::from_secs(1);
                reported = false;
                read_results(&mut socket, snapshot, live, stop);
                live.store(false, Ordering::Relaxed);
            }
            Err(SocketError::Unsupported) => {
                gaanim_core::console::hint(
                    "This relay cannot push results; the presentation asks it every second. \
                     Deploy the current relay (`gaanim relay init --force`) to push them.",
                );
                return;
            }
            Err(SocketError::Failed(error)) => {
                if !reported {
                    gaanim_core::console::info(
                        "polls",
                        format!("asking the relay every second, its socket failed: {error}"),
                    );
                    reported = true;
                }
            }
        }
        let resume = Instant::now() + backoff;
        while Instant::now() < resume && !stop.load(Ordering::Relaxed) {
            std::thread::sleep(READ_TICK);
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// Read the results `socket` pushes until it closes, goes silent or the
/// presentation ends.
fn read_results(
    socket: &mut RelaySocket,
    snapshot: &Mutex<Option<Snapshot>>,
    live: &AtomicBool,
    stop: &AtomicBool,
) {
    use tungstenite::Message;
    let mut pinged = Instant::now();
    let mut heard = Instant::now();
    loop {
        if stop.load(Ordering::Relaxed) {
            let _ = socket.close(None);
            let _ = socket.flush();
            return;
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                heard = Instant::now();
                if text.as_str() == "pong" {
                    continue;
                }
                if let Ok(results) = serde_json::from_str::<Results>(text.as_str()) {
                    store(snapshot, Snapshot::from_results(results, heard));
                    live.store(true, Ordering::Relaxed);
                }
            }
            Ok(Message::Close(_)) => return,
            Ok(_) => heard = Instant::now(),
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(_) => return,
        }
        if heard.elapsed() > SILENCE {
            return;
        }
        if pinged.elapsed() > PING {
            pinged = Instant::now();
            if socket.send(Message::text("ping")).is_err() {
                return;
            }
        }
    }
}

/// The relay's `PUT poll` body that opens `poll`.
fn open_body(poll: &TimelinePoll) -> serde_json::Value {
    let mut body = serde_json::json!({
        "id": poll.id,
        "question": poll.question,
        "options": poll.options,
    });
    if let Some(quiz) = &poll.quiz {
        body["correct"] = match quiz.correct.as_slice() {
            [one] if !poll.multiple => (*one).into(),
            many => many.into(),
        };
        body["time"] = quiz.time.into();
        body["points"] = quiz.points.into();
    }
    if poll.multiple {
        body["multiple"] = true.into();
    }
    if let Some(image) = &poll.image {
        body["image"] = image.hash.clone().into();
    }
    body
}

/// Make `latest` the snapshot the scene reads, numbered after the last one.
/// Answers to polls it does not repeat carry over, within the same game.
fn store(snapshot: &Mutex<Option<Snapshot>>, mut latest: Snapshot) {
    if let Ok(mut snapshot) = snapshot.lock() {
        if let Some(previous) = snapshot.as_ref() {
            latest.version = previous.version + 1;
            if previous.game == latest.game {
                for (poll, answers) in &previous.answers {
                    latest
                        .answers
                        .entry(poll.clone())
                        .or_insert_with(|| answers.clone());
                }
            }
        }
        *snapshot = Some(latest);
    }
}

// ---------------------------------------------------------------------------
// Presenter View
// ---------------------------------------------------------------------------

use crate::presenter::AudienceView;

impl AudiencePolls {
    /// Whether a relay session runs: votes arrive from its thread.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn connected(&self) -> bool {
        self.client.is_some()
    }

    /// `snapshot` for Presenter View. Like the relay with phones, it sends
    /// every poll's answers when Presenter View just said hello or a game
    /// started, and otherwise only the polls whose answers changed: they
    /// are most of an answer's size and grow with every question.
    fn share(&mut self, snapshot: &Snapshot, everything: bool) -> SharedSnapshot {
        let (game, sent) = &self.shared_answers;
        let everything = everything || *game != snapshot.game;
        let shared =
            snapshot.share(|poll| everything || sent.get(poll) != snapshot.answers.get(poll));
        self.shared_answers = (snapshot.game.clone(), snapshot.answers.clone());
        shared
    }

    /// The audience while a presentation with polls runs.
    pub(crate) fn view(&self) -> Option<AudienceView> {
        let client = self.client.as_ref()?;
        let snapshot = client.snapshot();
        Some(AudienceView {
            code: client.session.code.clone(),
            connected: snapshot.as_ref().map(|snapshot| snapshot.connected),
            players: snapshot.as_ref().map_or(0, |snapshot| snapshot.players),
            leaderboard: snapshot
                .map(|snapshot| {
                    snapshot
                        .leaderboard
                        .into_iter()
                        .map(|(name, score)| (name.to_string(), score))
                        .collect()
                })
                .unwrap_or_default(),
            confirm_reset: self.reset_armed(),
            gate: self.gate.clone(),
            download: cfg!(target_arch = "wasm32"),
            #[cfg(target_arch = "wasm32")]
            results: None,
            #[cfg(not(target_arch = "wasm32"))]
            results: client.results.as_ref().map(|root| {
                let saved = client
                    .saved
                    .lock()
                    .map(|saved| saved.clone())
                    .unwrap_or_default();
                crate::presenter::ResultsView {
                    folder: saved.folder.unwrap_or_else(|| root.clone()),
                    saved: saved.at.map(|at| at.elapsed()),
                    error: saved.error,
                }
            }),
        })
    }

    /// Save the game's results now.
    pub(crate) fn save_results(&self) {
        if let Some(client) = &self.client {
            client.send(Command::SaveResults);
        }
    }

    /// Remove a player and block its phone.
    pub(crate) fn kick(&self, name: &str) {
        if let Some(client) = &self.client {
            client.send(Command::Kick(name.to_string()));
        }
    }

    fn reset_armed(&self) -> bool {
        self.confirm_reset
            .is_some_and(|pressed| pressed.elapsed() < CONFIRM_RESET)
    }

    /// "New game": the first press asks for a second one, which erases every
    /// vote, answer and player. Returns whether the game was reset, so the
    /// presentation goes back to where the game begins.
    pub(crate) fn press_reset(&mut self) -> bool {
        if self.client.is_none() {
            return false;
        }
        if !self.reset_armed() {
            self.confirm_reset = Some(Instant::now());
            gaanim_core::console::info("polls", "press again (R) to start a new game");
            return false;
        }
        self.confirm_reset = None;
        if let Some(client) = &mut self.client {
            client.send(Command::Reset);
            // Quizzes revealed before can be revealed again, and the relay
            // forgot the stage.
            client.revealed.clear();
            client.stage = None;
            client.after_reset = true;
            gaanim_core::console::success("polls", "started a new game");
        }
        true
    }

    pub(crate) fn cancel_reset(&mut self) {
        self.confirm_reset = None;
    }
}

#[cfg(not(target_arch = "wasm32"))]
/// `gaanim relay reset`: start a new game on the session of the project
/// that `path` (a script or a folder) belongs to. Returns the session code.
pub fn reset_relay_session(path: &std::path::Path) -> Result<String, String> {
    let directory = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or_else(
                || std::path::PathBuf::from("."),
                std::path::Path::to_path_buf,
            )
    };
    let directory = directory.canonicalize().unwrap_or(directory);
    let (scope, project_relay) = gaanim_project::relay::scope_of(&directory);
    let (relay, _) = gaanim_project::relay::resolve(project_relay.as_deref())
        .ok_or_else(|| "no relay is set: run `gaanim relay use <URL>`".to_string())?;
    let session = gaanim_project::relay::session_for(&scope)?;
    RelayApi::new(&format!("{relay}/s/{}", session.code), &session.key)?
        .post("reset", serde_json::json!({}))?;
    Ok(session.code)
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
            multiple: false,
            image: None,
        }
    }

    /// A timeline with a stop at 1 s that advances once 3 players joined,
    /// in an app presenting live results.
    fn gated_app() -> App {
        use gaanim_timeline::timeline::{GateCondition, SegmentMetadata, SegmentStop, StopGate};
        let mut timeline = Timeline::new();
        timeline.cached_duration = 3.0;
        timeline.set_segments(vec![SegmentMetadata {
            id: 1,
            name: "lobby".into(),
            notes: None,
            start_time: 0.0,
            end_time: 3.0,
            stops: vec![SegmentStop {
                name: None,
                time: 1.0,
                ambient: None,
            }],
        }]);
        timeline.set_stop_gates(vec![StopGate {
            time: 1.0,
            until: GateCondition::Players { count: 3 },
        }]);
        let mut app = App::new();
        app.insert_resource(timeline)
            .insert_resource(PresentationMode { active: true })
            .insert_resource(PollResults {
                live: true,
                ..Default::default()
            })
            .init_resource::<AudiencePolls>()
            .add_systems(Update, stop_gate_system);
        app
    }

    fn frame(app: &mut App, time: f64, playing: bool) {
        let mut timeline = app.world_mut().resource_mut::<Timeline>();
        timeline.current_time = time;
        timeline.is_playing = playing;
        app.update();
    }

    /// The relay's answer for one game: `counts` and `answers` by poll.
    fn relay_answer(polls: serde_json::Value) -> Snapshot {
        let results = serde_json::json!({
            "polls": polls,
            "players": [{"name": "Ana", "score": 900}, {"name": "Bruno", "score": 0}],
            "playerCount": 2,
            "audience": [{"name": "Ana", "joined": 1000.0}, {"name": "Bruno", "joined": 2000.0}],
            "now": 5000.0,
            "latest": "q1",
            "game": "g1",
        });
        Snapshot::from_results(serde_json::from_value(results).unwrap(), Instant::now())
    }

    #[test]
    fn presenter_view_gets_each_polls_answers_once_and_keeps_them() {
        let q0 = serde_json::json!({"counts": [1, 1], "answers": [
            {"name": "Ana", "options": [0]}, {"name": "Bruno", "options": [1]}]});
        let first = relay_answer(serde_json::json!({"q0": q0}));
        let mut second = relay_answer(serde_json::json!({"q0": q0, "q1": {
            "counts": [0, 1], "answers": [{"name": "Ana", "options": [1]}]}}));
        second.version = 1;

        let mut audience = AudiencePolls::default();
        let hello = audience.share(&first, true);
        let next = audience.share(&second, false);
        assert_eq!(hello.answers.keys().collect::<Vec<_>>(), ["q0"]);
        assert_eq!(
            next.answers.keys().collect::<Vec<_>>(),
            ["q1"],
            "q0's answers did not change"
        );
        // It travels as JSON.
        let next: SharedSnapshot =
            serde_json::from_str(&serde_json::to_string(&next).unwrap()).unwrap();

        let following = Mutex::new(None);
        store(&following, Snapshot::from_shared(hello, Instant::now()));
        store(&following, Snapshot::from_shared(next, Instant::now()));
        let results = results_of(following.lock().unwrap().clone().unwrap());
        assert_eq!(results.answers["q0"]["Bruno"].first(), Some(1));
        assert_eq!(results.answers["q1"]["Ana"].first(), Some(1));
        assert_eq!(results.counts["q1"], vec![0, 1]);
        assert_eq!(results.players, 2);
        assert_eq!(results.leaderboard[0], (Arc::from("Ana"), 900));
        // Ana joined 4 s before the relay answered.
        assert!((results.audience[0].1 - 4.0).abs() < 0.5, "{results:?}");
    }

    #[test]
    fn presenter_view_shows_the_results_the_audience_page_passes_on() {
        let mut app = App::new();
        app.insert_resource(Timeline::new())
            .insert_resource(PresentationMode { active: true })
            .insert_resource(PresenterLink::new(LinkRole::Presenter))
            .init_resource::<crate::AudienceBlank>()
            .init_resource::<PollResults>()
            .init_resource::<AudiencePolls>()
            .add_systems(
                Update,
                (
                    crate::presenter_link::apply_link_messages_system,
                    audience_poll_system,
                )
                    .chain(),
            );
        let pass_on = |app: &mut App, live: bool, snapshot: Option<SharedSnapshot>| {
            let message =
                serde_json::json!({"type": "results", "live": live, "snapshot": snapshot});
            app.world_mut()
                .resource_mut::<PresenterLink>()
                .receive(message.to_string());
            app.update();
        };
        let answer = relay_answer(serde_json::json!({"q0": {"counts": [3, 2]}}));
        let shared = AudiencePolls::default().share(&answer, true);
        pass_on(&mut app, true, Some(shared));
        let results = app.world().resource::<PollResults>();
        assert!(results.live);
        assert_eq!(results.counts["q0"], vec![3, 2]);
        assert_eq!(results.players, 2);

        pass_on(&mut app, false, None);
        assert_eq!(
            *app.world().resource::<PollResults>(),
            PollResults::default(),
            "back to the rehearsal once the votes stop"
        );
    }

    #[test]
    fn presenter_view_leaves_gated_stops_to_the_audience_page() {
        let mut app = gated_app();
        app.insert_resource(PresenterLink::new(LinkRole::Presenter));
        frame(&mut app, 0.9, true);
        frame(&mut app, 1.0, false);
        app.world_mut().resource_mut::<PollResults>().players = 3;
        app.update();
        assert!(!app.world().resource::<Timeline>().is_playing);
        assert_eq!(app.world().resource::<AudiencePolls>().gate, None);
    }

    #[test]
    fn a_gated_stop_reached_going_forward_advances_once_the_audience_is_there() {
        let mut app = gated_app();
        frame(&mut app, 0.9, true);
        frame(&mut app, 1.0, false);
        assert!(!app.world().resource::<Timeline>().is_playing);
        assert_eq!(
            app.world().resource::<AudiencePolls>().gate.as_deref(),
            Some("Advances by itself: 0/3 players")
        );
        app.world_mut().resource_mut::<PollResults>().players = 3;
        app.update();
        // Advancing from a paused stop resumes playback.
        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn a_presentation_starting_on_a_gated_stop_advances_from_it() {
        let mut app = gated_app();
        app.world_mut().resource_mut::<PollResults>().players = 3;
        // The editor was elsewhere before presenting.
        app.world_mut().resource_mut::<PresentationMode>().active = false;
        frame(&mut app, 2.5, false);
        app.world_mut().resource_mut::<PresentationMode>().active = true;
        frame(&mut app, 1.0, false);
        assert!(app.world().resource::<Timeline>().is_playing);
    }

    #[test]
    fn going_back_to_a_gated_stop_does_not_advance_it() {
        let mut app = gated_app();
        app.world_mut().resource_mut::<PollResults>().players = 5;
        frame(&mut app, 2.0, false);
        frame(&mut app, 1.0, false);
        app.update();
        assert!(!app.world().resource::<Timeline>().is_playing);
        assert_eq!(
            app.world().resource::<AudiencePolls>().gate.as_deref(),
            Some("Would advance by itself: 5/3 players")
        );
    }

    #[test]
    fn a_quiz_is_revealed_once_the_playhead_leaves_its_stop() {
        // Resting on the stop the reveal shares keeps the quiz open.
        assert!(!passed(19.5, 19.5));
        assert!(!passed(19.5, 18.0));
        // The first frame after advancing reveals it.
        assert!(passed(19.5, 19.5 + 1.0 / 60.0));
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

    /// Talks to a real relay: `npm run dev` in `gaanim_project/relay`, then
    /// `GAANIM_TEST_RELAY=http://127.0.0.1:8787 cargo test -p gaanim_editor
    /// polls -- --ignored`.
    #[test]
    #[ignore = "needs a running relay in GAANIM_TEST_RELAY"]
    #[allow(
        clippy::result_large_err,
        reason = "ureq's error, unwrapped by the test"
    )]
    fn votes_reach_the_presentation_through_a_relay() {
        let relay = std::env::var("GAANIM_TEST_RELAY").expect("set GAANIM_TEST_RELAY");
        let code = "TSTR2A";
        let session_url = format!("{relay}/s/{code}");
        let kept = std::env::temp_dir().join(format!("gaanim-kept-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&kept);
        let mut client = PollClient::start(
            PollSessionInfo {
                relay: Some(relay),
                code: code.into(),
                lobby: true,
                game_segment: None,
                teams: None,
                ask: Some(gaanim_timeline::timeline::AskInfo {
                    label: "Código".into(),
                    required: false,
                }),
            },
            Some(kept.clone()),
        )
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
        // The question as phone `voter` sees it, with its own choice.
        let seen_by = |voter: char| -> serde_json::Value {
            phone
                .get(&format!(
                    "{session_url}/poll?voter={}",
                    voter.to_string().repeat(32)
                ))
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
        // From here on the relay pushes the results on the presentation's
        // socket, and the client stops asking for them.
        wait_for(&client, |_| client.socket.live.load(Ordering::Relaxed));
        // A relay without the socket (older than API 6) answers 404: the
        // client then keeps asking over HTTP.
        let missing = format!(
            "{}/presenter/missing",
            session_url.replacen("http", "ws", 1)
        );
        assert!(matches!(
            connect_results(&missing, &format!("Bearer {}", "0".repeat(64))),
            Err(SocketError::Unsupported)
        ));
        let first = poll("p0-test", "¿Primera?");
        client.show(Some(&first));
        wait_for(&client, |results| results.counts.contains_key("p0-test"));
        assert_eq!(current()["question"], "¿Primera?");
        vote("p0-test", 'a', 0).unwrap();
        vote("p0-test", 'b', 1).unwrap();
        vote("p0-test", 'c', 1).unwrap();
        vote("p0-test", 'c', 0).unwrap();
        // Three phones, one of which changed its vote.
        let results = wait_for(&client, |results| {
            results
                .counts
                .get("p0-test")
                .is_some_and(|votes| votes == &[2, 1])
        });
        assert_eq!(results.counts["p0-test"], [2, 1]);
        // Each phone learns its own vote from the relay.
        assert_eq!(seen_by('c')["chosen"], 0);
        assert_eq!(seen_by('b')["chosen"], 1);
        assert_eq!(seen_by('d')["chosen"], serde_json::Value::Null);
        assert_eq!(current()["lobby"], true);

        // A quiz: players join, answer against the clock, and the reveal
        // builds the leaderboard.
        let mut quiz = poll("q1-test", "¿2 + 2?");
        quiz.options = vec!["3".into(), "4".into()];
        quiz.quiz = Some(gaanim_timeline::timeline::TimelineQuiz {
            correct: vec![1],
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
        // The audience lists players in the order they joined.
        let results = wait_for(&client, |results| results.audience.len() == 2);
        let names: Vec<&str> = results
            .audience
            .iter()
            .map(|(name, _)| name.as_ref())
            .collect();
        assert_eq!(names, ["Ana", "Beto"]);
        assert!(results.audience[0].1 < 30.0);
        vote("q1-test", 'a', 1).unwrap();
        vote("q1-test", 'b', 0).unwrap();
        assert_eq!(seen_by('a')["chosen"], 1);
        client.reveal("q1-test");
        let results = wait_for(&client, |results| {
            results.players == 2
                && results
                    .leaderboard
                    .first()
                    .is_some_and(|(_, score)| *score > 0)
        });
        assert_eq!(results.leaderboard[0].0.as_ref(), "Ana");
        assert!(results.leaderboard[0].1 > 900);
        assert_eq!(results.leaderboard[1], (Arc::from("Beto"), 0));
        wait_for(&client, |_| vote("q1-test", 'a', 1).is_err());

        // The game's results are kept, with what joining asked.
        let deadline = Instant::now() + Duration::from_secs(15);
        let players = loop {
            let players = std::fs::read_dir(&kept)
                .ok()
                .and_then(|mut games| games.next())
                .and_then(|game| {
                    std::fs::read_to_string(game.ok()?.path().join("jugadores.csv")).ok()
                });
            if let Some(players) = players.filter(|players| players.contains("4 ✓")) {
                break players;
            }
            assert!(
                Instant::now() < deadline,
                "no results were kept in {}",
                kept.display()
            );
            std::thread::sleep(Duration::from_millis(200));
        };
        assert!(players.contains("Puesto,Apodo,Código,"), "{players}");
        assert!(players.contains("1,Ana,"), "{players}");

        // Coming back to the first poll keeps its votes.
        client.show(Some(&first));
        let results = wait_for(&client, |_| current()["id"] == "p0-test");
        assert_eq!(results.counts["p0-test"], [2, 1]);

        client.send(Command::Kick("beto".into()));
        let results = wait_for(&client, |results| results.players == 1);
        assert_eq!(results.audience.len(), 1);

        // A new game forgets every vote: phones see no choice of theirs.
        client.send(Command::Reset);
        wait_for(&client, |results| {
            results.players == 0
                && results
                    .counts
                    .get("p0-test")
                    .is_some_and(|votes| votes == &[0, 0])
        });
        assert_eq!(seen_by('c')["chosen"], serde_json::Value::Null);
        // The lobby outlives the reset.
        assert_eq!(current()["lobby"], true);
        client.show(None);
        wait_for(&client, |_| current()["open"] == false);
        let _ = std::fs::remove_dir_all(&kept);
    }
}
