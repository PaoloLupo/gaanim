//! The relay client of the web player.
//!
//! The page makes the requests (`fetch`) and holds the results socket
//! (`WebSocket`) through [`WebRelayHooks`], and hands back what it hears
//! through [`response`], [`socket_message`] and [`socket_state`]. Each frame
//! [`RelaySession::pump`] does what the native thread does in its loop:
//! keep the relay's open poll on the presentation's, send one-off requests
//! in order, retry what failed, and ask for the results while the socket
//! does not push them. A browser cannot set headers on a WebSocket, so the
//! page offers the presenter key as a subprotocol, which relays of API 13
//! accept.

use std::collections::HashSet;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};

use bevy::platform::time::Instant;
use gaanim_timeline::timeline::TimelinePoll;

use super::{Command, REFRESH, Results, Snapshot, open_body, store};

/// What the page does for the relay client.
pub struct WebRelayHooks {
    /// The presenter key of session `code` on `relay`: the one the page's
    /// link brought (`#clave=`), or one this browser keeps for the session.
    pub key: fn(relay: &str, code: &str) -> String,
    /// Send a request; its answer comes back through [`response`] with
    /// `id`. Id 0 needs no answer.
    pub request: fn(id: u32, method: &str, url: &str, key: &str, content_type: &str, body: Vec<u8>),
    /// Keep the results socket at `url` open with `key`, reconnecting.
    pub open_socket: fn(url: &str, key: &str),
    pub close_socket: fn(),
    /// Offer a file to save.
    pub download: fn(name: &str, bytes: Vec<u8>),
    /// Tell the speaker something, briefly.
    pub notify: fn(message: &str),
}

/// Set once by the web player.
pub static HOOKS: OnceLock<WebRelayHooks> = OnceLock::new();

/// What the page heard since the last frame.
#[derive(Default)]
struct Heard {
    responses: Vec<(u32, u16, String)>,
    messages: Vec<String>,
    /// Whether the socket is open, when that changed.
    live: Option<bool>,
}

static HEARD: Mutex<Heard> = Mutex::new(Heard {
    responses: Vec::new(),
    messages: Vec::new(),
    live: None,
});

fn heard() -> std::sync::MutexGuard<'static, Heard> {
    HEARD
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The answer to request `id`: its HTTP status (0 when it never arrived)
/// and body.
pub fn response(id: u32, status: u16, body: String) {
    if id != 0 {
        heard().responses.push((id, status, body));
    }
}

/// A message the results socket pushed.
pub fn socket_message(text: String) {
    heard().messages.push(text);
}

/// The results socket closed (`false`); it opens again by itself.
pub fn socket_state(live: bool) {
    heard().live = Some(live);
}

/// What a request in flight was for.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Purpose {
    Upload,
    Open,
    Close,
    /// The first one-off request in the queue.
    Queued,
    Results,
    Report,
}

/// One relay session, driven each frame.
pub(super) struct RelaySession {
    session_url: String,
    key: String,
    /// In a mutex only because a resource must be `Sync`.
    commands: Mutex<Receiver<Command>>,
    snapshot: Arc<Mutex<Option<Snapshot>>>,
    /// What the relay should show, and whether it already does.
    wanted: Option<TimelinePoll>,
    synced: bool,
    /// One-off requests still to send, in order.
    queued: Vec<Command>,
    /// Pictures the relay already has.
    uploaded: HashSet<String>,
    /// Presenter View asked for the results.
    save_now: bool,
    in_flight: Option<(u32, Purpose)>,
    next_id: u32,
    /// After a failure, requests wait until then.
    retry_at: Option<Instant>,
    /// When the results were last asked for over HTTP.
    asked: Option<Instant>,
    /// Results arrive on the socket.
    live: bool,
    /// The relay could not be reached; said once until it can again.
    offline: bool,
}

impl RelaySession {
    pub(super) fn start(
        relay: &str,
        code: &str,
        commands: Receiver<Command>,
        snapshot: Arc<Mutex<Option<Snapshot>>>,
    ) -> Result<Self, String> {
        let hooks = HOOKS
            .get()
            .ok_or_else(|| "the web player has no relay client".to_string())?;
        let key = (hooks.key)(relay, code);
        let session_url = format!("{relay}/s/{code}");
        *heard() = Heard::default();
        let socket = format!(
            "{}/presenter",
            session_url
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        );
        (hooks.open_socket)(&socket, &key);
        Ok(Self {
            session_url,
            key,
            commands: Mutex::new(commands),
            snapshot,
            wanted: None,
            synced: true,
            queued: Vec::new(),
            uploaded: HashSet::new(),
            save_now: false,
            in_flight: None,
            next_id: 1,
            retry_at: None,
            asked: None,
            live: false,
            offline: false,
        })
    }

    fn hooks() -> &'static WebRelayHooks {
        HOOKS.get().expect("set before a session starts")
    }

    fn send(
        &mut self,
        purpose: Purpose,
        method: &str,
        path: &str,
        body: Option<serde_json::Value>,
    ) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let (content_type, bytes) = match body {
            Some(body) => ("application/json", body.to_string().into_bytes()),
            None => ("", Vec::new()),
        };
        let url = format!("{}/{path}", self.session_url);
        (Self::hooks().request)(id, method, &url, &self.key, content_type, bytes);
        self.in_flight = Some((id, purpose));
    }

    /// Do what one turn of the native thread's loop does, without waiting:
    /// take the presentation's commands and the page's answers, then send
    /// the next request when none is in flight.
    pub(super) fn pump(&mut self) {
        let commands: Vec<Command> = match self.commands.get_mut() {
            Ok(receiver) => receiver.try_iter().collect(),
            Err(poisoned) => poisoned.into_inner().try_iter().collect(),
        };
        for command in commands {
            match command {
                Command::Open(poll) => {
                    self.wanted = Some(poll);
                    self.synced = false;
                }
                Command::Close => {
                    self.wanted = None;
                    self.synced = false;
                }
                Command::SaveResults => self.save_now = true,
                Command::Stage(stage) => {
                    self.queued
                        .retain(|queued| !matches!(queued, Command::Stage(_)));
                    self.queued.push(Command::Stage(stage));
                }
                command => self.queued.push(command),
            }
        }

        let Heard {
            responses,
            messages,
            live,
        } = std::mem::take(&mut *heard());
        if let Some(live) = live {
            self.live = live;
        }
        for text in messages {
            if let Ok(results) = serde_json::from_str::<Results>(&text) {
                store(
                    &self.snapshot,
                    Snapshot::from_results(results, Instant::now()),
                );
                self.live = true;
            }
        }
        for (id, status, body) in responses {
            if let Some((expected, purpose)) = self.in_flight
                && expected == id
            {
                self.in_flight = None;
                self.answered(purpose, status, &body);
            }
        }

        if self.in_flight.is_some() || self.retry_at.is_some_and(|at| Instant::now() < at) {
            return;
        }
        self.retry_at = None;
        if !self.synced {
            match self.wanted.clone() {
                // A picture goes up once, before its question.
                Some(poll) => match &poll.image {
                    Some(image) if !self.uploaded.contains(&image.hash) => {
                        let id = self.next_id;
                        self.next_id = self.next_id.wrapping_add(1).max(1);
                        let url = format!("{}/image/{}", self.session_url, image.hash);
                        (Self::hooks().request)(
                            id,
                            "PUT",
                            &url,
                            &self.key,
                            &image.mime,
                            image.bytes.to_vec(),
                        );
                        self.in_flight = Some((id, Purpose::Upload));
                    }
                    _ => self.send(Purpose::Open, "PUT", "poll", Some(open_body(&poll))),
                },
                None => self.send(Purpose::Close, "DELETE", "poll", None),
            }
            return;
        }
        if let Some(command) = self.queued.first() {
            let (path, body) = match command {
                Command::Reveal(id) => ("reveal", serde_json::json!({ "id": id })),
                Command::Kick(name) => ("kick", serde_json::json!({ "name": name })),
                Command::Reset => ("reset", serde_json::json!({})),
                Command::Lobby => ("lobby", serde_json::json!({ "open": true })),
                Command::Teams(teams) => (
                    "teams",
                    serde_json::json!({
                        "names": teams.names,
                        "colors": teams.colors,
                        "choose": teams.choose,
                    }),
                ),
                Command::Stage(stage) => ("stage", serde_json::json!({ "stage": stage })),
                Command::Ask(ask) => (
                    "ask",
                    serde_json::json!({ "label": ask.label, "required": ask.required }),
                ),
                Command::Open(_) | Command::Close | Command::SaveResults => {
                    self.queued.remove(0);
                    return;
                }
            };
            self.send(Purpose::Queued, "POST", path, Some(body));
            return;
        }
        if self.save_now {
            self.send(Purpose::Report, "GET", "report", None);
            return;
        }
        if !self.live && self.asked.is_none_or(|asked| asked.elapsed() >= REFRESH) {
            self.asked = Some(Instant::now());
            self.send(Purpose::Results, "GET", "results", None);
        }
    }

    fn answered(&mut self, purpose: Purpose, status: u16, body: &str) {
        let ok = (200..300).contains(&status);
        if purpose == Purpose::Report {
            self.save_now = false;
            self.download_report(ok, status, body);
            return;
        }
        if !ok {
            // A player already gone, or a quiz the relay does not know:
            // nothing to retry.
            if purpose == Purpose::Queued && status == 404 {
                self.queued.remove(0);
                return;
            }
            self.unreachable(status, body);
            return;
        }
        if self.offline {
            self.offline = false;
            gaanim_core::console::info("polls", "the relay is reachable again");
        }
        match purpose {
            Purpose::Upload => {
                if let Some(image) = self.wanted.as_ref().and_then(|poll| poll.image.as_ref()) {
                    self.uploaded.insert(image.hash.clone());
                }
            }
            Purpose::Open | Purpose::Close => self.synced = true,
            Purpose::Queued => {
                let done = self.queued.remove(0);
                // A reset forgets the open question: open it again.
                if matches!(done, Command::Reset) && self.wanted.is_some() {
                    self.synced = false;
                }
            }
            Purpose::Results => {
                if let Ok(results) = serde_json::from_str::<Results>(body) {
                    store(
                        &self.snapshot,
                        Snapshot::from_results(results, Instant::now()),
                    );
                }
            }
            Purpose::Report => {}
        }
    }

    /// A request failed: try again after [`REFRESH`], and tell the speaker
    /// why, once.
    fn unreachable(&mut self, status: u16, body: &str) {
        self.retry_at = Some(Instant::now() + REFRESH);
        if self.offline {
            return;
        }
        self.offline = true;
        let reason = serde_json::from_str::<serde_json::Value>(body)
            .ok()
            .and_then(|body| body["error"].as_str().map(str::to_owned));
        let relay = self
            .session_url
            .rsplit_once("/s/")
            .map_or(self.session_url.as_str(), |(relay, _)| relay);
        let message = match (status, reason) {
            (0, _) => format!(
                "No se puede conectar con el relay ({relay}). Si es anterior a Gaanim 0.8, \
                 actualízalo para el reproductor web: `gaanim relay init --force <su carpeta>` y \
                 `npx wrangler deploy`. Se reintenta solo."
            ),
            (403, _) => "Otra computadora presenta esta sesión con otra clave. Para presentarla \
                 aquí, abre el enlace que da `gaanim relay key <archivo.gaanim>` en esa \
                 computadora."
                .to_string(),
            (status, Some(reason)) => format!("El relay respondió {status}: {reason}."),
            (status, None) => format!("El relay respondió {status}."),
        };
        gaanim_core::console::warn("polls", message.clone());
        (Self::hooks().notify)(&message);
    }

    fn download_report(&mut self, ok: bool, status: u16, body: &str) {
        let hooks = Self::hooks();
        if !ok {
            (hooks.notify)(&format!(
                "No se pudieron descargar los resultados (el relay respondió {status})."
            ));
            return;
        }
        let archive = serde_json::from_str::<crate::poll_report::Report>(body)
            .map_err(|error| format!("the relay's report is not valid: {error}"))
            .and_then(|report| {
                if report.is_empty() {
                    Err("todavía nadie respondió".to_string())
                } else {
                    report.archive()
                }
            });
        match archive {
            Ok((name, bytes)) => (hooks.download)(&name, bytes),
            Err(error) => (hooks.notify)(&format!("No hay resultados que descargar: {error}.")),
        }
    }
}

impl Drop for RelaySession {
    /// The presentation ended: stop taking votes, and phones say goodbye.
    /// The page sends these without waiting for answers.
    fn drop(&mut self) {
        let hooks = Self::hooks();
        (hooks.close_socket)();
        if self.wanted.is_some() {
            let url = format!("{}/poll", self.session_url);
            (hooks.request)(0, "DELETE", &url, &self.key, "", Vec::new());
        }
        let url = format!("{}/stage", self.session_url);
        let body = serde_json::json!({ "stage": "end" })
            .to_string()
            .into_bytes();
        (hooks.request)(0, "POST", &url, &self.key, "application/json", body);
    }
}
