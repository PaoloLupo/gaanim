//! Audience polls during a presentation.
//!
//! A scene authors its polls with `scene.poll` and draws them itself: the
//! QR code, the question, and whatever follows the poll's values. While a
//! presentation runs, this module opens on the relay the poll whose window
//! holds the playhead, closes it when the playhead leaves, and copies every
//! poll's counts into [`PollResults`], which the scene's poll values and
//! bars read. A thread talks to the relay, so a frame never waits on the
//! network; outside a presentation the results stay empty and the scene
//! shows its preview counts.

use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::prelude::*;
use gaanim_animation::polls::PollResults;
use gaanim_timeline::timeline::{PollSessionInfo, Timeline, TimelinePoll};
use serde::Deserialize;

use crate::PresentationMode;

/// How often the counts are read while presenting.
const REFRESH: Duration = Duration::from_millis(1000);
/// How long a request to the relay may take.
const TIMEOUT: Duration = Duration::from_secs(5);

type Counts = HashMap<Arc<str>, Vec<u32>>;

pub(crate) struct AudiencePollsPlugin;

impl Plugin for AudiencePollsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AudiencePolls>()
            .init_resource::<PollResults>()
            .add_systems(
                Update,
                audience_poll_system.in_set(gaanim_scene::hierarchy::SceneSet::Input),
            );
    }
}

/// The relay session of the presentation, while one runs.
#[derive(Resource, Default)]
pub(crate) struct AudiencePolls {
    client: Option<PollClient>,
    /// The session a relay could not be used for, reported once.
    unusable: Option<PollSessionInfo>,
}

/// Keep the relay's open poll on the one the playhead is in, and publish
/// the counts of every poll to the scene.
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
        // open question, and the scene goes back to its preview counts.
        polls.client = None;
        if !results.0.is_empty() {
            results.0.clear();
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
    client.show(timeline.poll_open_at(timeline.current_time));
    if let Some(counts) = client.counts()
        && counts != results.0
    {
        results.0 = counts;
    }
}

// ---------------------------------------------------------------------------
// Relay client
// ---------------------------------------------------------------------------

enum Command {
    Open(TimelinePoll),
    Close,
}

struct PollClient {
    session: PollSessionInfo,
    /// The poll the relay was last told to open.
    open: Option<String>,
    commands: Sender<Command>,
    counts: Arc<Mutex<Option<Counts>>>,
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
        let counts = Arc::new(Mutex::new(None));
        let (commands, receiver) = mpsc::channel();
        let thread_counts = counts.clone();
        std::thread::Builder::new()
            .name("gaanim-polls".into())
            .spawn(move || run_relay_session(api, receiver, thread_counts))
            .map_err(|error| format!("could not start the poll client: {error}"))?;
        gaanim_core::console::info("polls", format!("votes go to {relay}/s/{}", session.code));
        Ok(Self {
            session,
            open: None,
            commands,
            counts,
        })
    }

    fn show(&mut self, poll: Option<&TimelinePoll>) {
        let id = poll.map(|poll| poll.id.clone());
        if id == self.open {
            return;
        }
        self.open = id;
        // The thread only ends with the client, so the send cannot fail.
        let _ = self.commands.send(match poll {
            Some(poll) => Command::Open(poll.clone()),
            None => Command::Close,
        });
    }

    /// Every poll's counts, once the relay reported them.
    fn counts(&self) -> Option<Counts> {
        self.counts.lock().ok()?.clone()
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
}

#[derive(Deserialize)]
struct PollCounts {
    #[serde(default)]
    counts: Vec<u32>,
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

    fn open(&self, poll: &TimelinePoll) -> Result<(), String> {
        let body = serde_json::json!({
            "id": poll.id,
            "question": poll.question,
            "options": poll.options,
        });
        self.request("PUT", "poll")
            .send_json(body)
            .map_err(describe)?;
        Ok(())
    }

    fn close(&self) -> Result<(), String> {
        self.request("DELETE", "poll").call().map_err(describe)?;
        Ok(())
    }

    fn counts(&self) -> Result<Counts, String> {
        let response = self.request("GET", "results").call().map_err(describe)?;
        let results: Results = response.into_json().map_err(|error| error.to_string())?;
        Ok(results
            .polls
            .into_iter()
            .map(|(id, poll)| (Arc::from(id), poll.counts))
            .collect())
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

/// The relay thread: open and close polls as commanded and read every
/// poll's counts each [`REFRESH`], retrying what failed on the next tick.
fn run_relay_session(
    api: RelayApi,
    commands: Receiver<Command>,
    counts: Arc<Mutex<Option<Counts>>>,
) {
    // What the relay should show, and whether it already does.
    let mut wanted: Option<TimelinePoll> = None;
    let mut synced = true;
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
        // Only the latest command matters after a burst of steps.
        while let Some(command) = next.take().or_else(|| commands.try_recv().ok()) {
            wanted = match command {
                Command::Open(poll) => Some(poll),
                Command::Close => None,
            };
            synced = false;
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
        let result = api.counts().map(|latest| {
            if let Ok(mut counts) = counts.lock() {
                *counts = Some(latest);
            }
        });
        report(&result);
    }
    // The presentation ended: stop taking votes.
    if wanted.is_some() {
        let _ = api.close();
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
        }
    }

    /// Wait until `check` accepts the client's counts.
    fn wait_for(client: &PollClient, check: impl Fn(&Counts) -> bool) -> Counts {
        for _ in 0..100 {
            if let Some(counts) = client.counts().filter(|counts| check(counts)) {
                return counts;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("the relay did not answer in time: {:?}", client.counts());
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
        let vote = |poll: &str, voter: char, option: u32| {
            phone
                .post(&format!("{session_url}/vote"))
                .send_json(serde_json::json!({
                    "poll": poll, "option": option, "voter": voter.to_string().repeat(32),
                }))
        };
        let current = || -> serde_json::Value {
            phone
                .get(&format!("{session_url}/poll"))
                .call()
                .unwrap()
                .into_json()
                .unwrap()
        };

        let first = poll("p0-test", "¿Primera?");
        client.show(Some(&first));
        wait_for(&client, |counts| counts.contains_key("p0-test"));
        assert_eq!(current()["question"], "¿Primera?");
        vote("p0-test", 'a', 0).unwrap();
        vote("p0-test", 'b', 1).unwrap();
        vote("p0-test", 'c', 1).unwrap();
        vote("p0-test", 'c', 0).unwrap();
        let counts = wait_for(&client, |counts| {
            counts
                .get("p0-test")
                .is_some_and(|votes| votes.iter().sum::<u32>() == 3)
        });
        assert_eq!(counts["p0-test"], [2, 1]);

        // The next poll starts from zero; coming back keeps the first one.
        client.show(Some(&poll("p1-test", "¿Segunda?")));
        wait_for(&client, |_| current()["id"] == "p1-test");
        assert!(vote("p0-test", 'a', 1).is_err());
        client.show(Some(&first));
        let counts = wait_for(&client, |_| current()["id"] == "p0-test");
        assert_eq!(counts["p0-test"], [2, 1]);

        client.show(None);
        wait_for(&client, |_| current()["open"] == false);
    }
}
