//! Keeps two players on the same moment of one presentation.
//!
//! A browser runs the web player's audience page and its Presenter View as
//! separate pages, each with its own world, so they cannot share the
//! timeline as the desktop windows do. Each page publishes its playhead when
//! something local moves it (a key, a click, the dock, the seek bar) and
//! applies what the other page publishes. The page carries the messages
//! (a `BroadcastChannel`); this module only reads and writes them.
//!
//! In a presentation with audience polls, only the audience page talks to
//! the relay: it tells Presenter View who is playing, and Presenter View
//! asks it to remove a player, start a new game or download the results.

use bevy::prelude::*;
use gaanim_timeline::timeline::Timeline;
use serde::{Deserialize, Serialize};

use crate::AudienceBlank;
use crate::presenter::AudienceView;

/// Largest jump, in seconds, that playback itself can explain between two
/// frames; a bigger one is a seek.
const SEEK_TOLERANCE: f64 = 0.25;
/// While playing, the Presenter View republishes its playhead this often, in
/// seconds, so the two clocks cannot drift apart.
const HEARTBEAT_SECONDS: f64 = 1.0;
/// A heartbeat corrects a page that has drifted further than this, in seconds.
const HEARTBEAT_TOLERANCE: f64 = 1.0 / 30.0;

/// Which page this player is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkRole {
    /// The slides the audience sees.
    #[default]
    Audience,
    /// The speaker's Presenter View.
    Presenter,
}

/// What both pages must agree on.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
struct LinkState {
    time: f64,
    playing: bool,
    blank: Blank,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Blank {
    None,
    Black,
    White,
}

impl From<AudienceBlank> for Blank {
    fn from(blank: AudienceBlank) -> Self {
        match blank {
            AudienceBlank::None => Self::None,
            AudienceBlank::Black => Self::Black,
            AudienceBlank::White => Self::White,
        }
    }
}

impl From<Blank> for AudienceBlank {
    fn from(blank: Blank) -> Self {
        match blank {
            Blank::None => Self::None,
            Blank::Black => Self::Black,
            Blank::White => Self::White,
        }
    }
}

/// What Presenter View asks of the audience page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "request", rename_all = "lowercase")]
pub(crate) enum AudienceRequest {
    Kick {
        name: String,
    },
    /// "New game", pressed once more.
    Reset,
    CancelReset,
    /// Download the game's results.
    SaveResults,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum LinkMessage {
    /// A Presenter View that just opened asks for the current state.
    Hello,
    /// The audience, while a presentation with polls runs.
    Audience {
        view: Option<AudienceView>,
    },
    Request {
        request: AudienceRequest,
    },
    /// The sender's playhead. A heartbeat only corrects drift.
    State {
        #[serde(flatten)]
        state: LinkState,
        #[serde(default)]
        heartbeat: bool,
    },
}

impl LinkState {
    /// The state `timeline` shows or, with a seek pending, is about to show.
    fn of(timeline: &Timeline, blank: AudienceBlank) -> Self {
        Self {
            time: timeline.seek_request.unwrap_or(timeline.current_time),
            playing: timeline.is_playing,
            blank: blank.into(),
        }
    }

    /// Whether going from `self` to `next` in `elapsed` seconds of playback
    /// needed something besides playback: a seek, play or pause, a blank.
    fn jumped_to(&self, next: &Self, elapsed: f64) -> bool {
        let expected = self.time + if self.playing { elapsed } else { 0.0 };
        self.playing != next.playing
            || self.blank != next.blank
            || (next.time - expected).abs() > SEEK_TOLERANCE
    }
}

/// The link of this page to the other one: messages in and out, and the
/// state both last agreed on.
#[derive(Resource, Debug, Default)]
pub struct PresenterLink {
    role: LinkRole,
    inbox: Vec<String>,
    outbox: Vec<String>,
    /// The state at the end of the last frame, or the one just applied.
    baseline: Option<LinkState>,
    /// Publish the state at the end of this frame even if nothing moved it.
    publish: bool,
    greeted: bool,
    since_heartbeat: f64,
    /// A Presenter View page said hello since the audience was last shared.
    peer_hello: bool,
    /// The audience as the audience page last told it (Presenter View).
    audience: Option<AudienceView>,
    /// What Presenter View asked (audience page).
    requests: Vec<AudienceRequest>,
}

impl PresenterLink {
    pub fn new(role: LinkRole) -> Self {
        Self {
            role,
            ..Default::default()
        }
    }

    pub fn role(&self) -> LinkRole {
        self.role
    }

    /// Queue a message from the other page.
    pub fn receive(&mut self, message: String) {
        self.inbox.push(message);
    }

    /// Messages for the other page, oldest first.
    pub fn take_outgoing(&mut self) -> Vec<String> {
        std::mem::take(&mut self.outbox)
    }

    fn send(&mut self, message: &LinkMessage) {
        if let Ok(text) = serde_json::to_string(message) {
            self.outbox.push(text);
        }
    }

    /// Tell Presenter View the audience (audience page).
    pub(crate) fn send_audience(&mut self, view: Option<AudienceView>) {
        self.send(&LinkMessage::Audience { view });
    }

    /// Whether a Presenter View page said hello since the last call.
    pub(crate) fn take_peer_hello(&mut self) -> bool {
        std::mem::take(&mut self.peer_hello)
    }

    /// The audience, as the audience page last told it (Presenter View).
    pub(crate) fn audience(&self) -> Option<&AudienceView> {
        self.audience.as_ref()
    }

    /// Ask the audience page for a change (Presenter View).
    pub(crate) fn request(&mut self, request: AudienceRequest) {
        self.send(&LinkMessage::Request { request });
    }

    /// What Presenter View asked since the last call (audience page).
    pub(crate) fn take_audience_requests(&mut self) -> Vec<AudienceRequest> {
        std::mem::take(&mut self.requests)
    }
}

/// Apply the other page's messages before playback advances this frame.
pub(crate) fn apply_link_messages_system(
    mut link: ResMut<PresenterLink>,
    timeline: Option<ResMut<Timeline>>,
    mut blank: ResMut<AudienceBlank>,
) {
    // Messages wait for the bundle: a state needs a timeline to land on.
    let Some(mut timeline) = timeline else {
        return;
    };
    for text in std::mem::take(&mut link.inbox) {
        let Ok(message) = serde_json::from_str::<LinkMessage>(&text) else {
            continue;
        };
        match message {
            LinkMessage::Hello => {
                link.publish = true;
                link.peer_hello = true;
            }
            LinkMessage::Audience { view } => link.audience = view,
            LinkMessage::Request { request } => link.requests.push(request),
            LinkMessage::State { state, heartbeat } => {
                if heartbeat
                    && state.playing == timeline.is_playing
                    && (state.time - timeline.current_time).abs() <= HEARTBEAT_TOLERANCE
                {
                    continue;
                }
                timeline.seek_request = Some(state.time);
                timeline.is_playing = state.playing;
                *blank = state.blank.into();
                link.baseline = Some(state);
            }
        }
    }
}

/// Publish the playhead when something on this page moved it.
pub(crate) fn publish_link_changes_system(
    mut link: ResMut<PresenterLink>,
    timeline: Option<Res<Timeline>>,
    blank: Res<AudienceBlank>,
    dt: Res<gaanim_animation::DeltaTime>,
) {
    let Some(timeline) = timeline else {
        return;
    };
    let local = LinkState::of(&timeline, *blank);
    let elapsed = dt.dt * timeline.playback_rate;
    if link.role == LinkRole::Presenter && !link.greeted {
        link.greeted = true;
        link.send(&LinkMessage::Hello);
    }
    let moved = link
        .baseline
        .is_some_and(|baseline| baseline.jumped_to(&local, elapsed));
    let mut heartbeat = false;
    if link.role == LinkRole::Presenter && local.playing {
        link.since_heartbeat += dt.dt;
        if link.since_heartbeat >= HEARTBEAT_SECONDS {
            link.since_heartbeat = 0.0;
            heartbeat = true;
        }
    } else {
        link.since_heartbeat = 0.0;
    }
    if moved || link.publish || heartbeat {
        link.publish = false;
        link.send(&LinkMessage::State {
            state: local,
            heartbeat: heartbeat && !moved,
        });
    }
    link.baseline = Some(local);
}

pub(crate) struct PresenterLinkPlugin;

impl Plugin for PresenterLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            apply_link_messages_system
                .run_if(resource_exists::<PresenterLink>)
                .in_set(gaanim_scene::hierarchy::SceneSet::Input)
                .before(gaanim_timeline::timeline_playback_system),
        )
        .add_systems(
            PostUpdate,
            publish_link_changes_system.run_if(resource_exists::<PresenterLink>),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(role: LinkRole) -> App {
        let mut app = App::new();
        app.insert_resource(PresenterLink::new(role))
            .insert_resource({
                let mut timeline = Timeline::default();
                timeline.cached_duration = 10.0;
                timeline
            })
            .init_resource::<AudienceBlank>()
            .init_resource::<gaanim_animation::DeltaTime>()
            .add_systems(Update, apply_link_messages_system)
            .add_systems(PostUpdate, publish_link_changes_system);
        app
    }

    fn sent(app: &mut App) -> Vec<LinkMessage> {
        app.world_mut()
            .resource_mut::<PresenterLink>()
            .take_outgoing()
            .iter()
            .map(|text| serde_json::from_str(text).expect("valid message"))
            .collect()
    }

    /// Carry `from`'s messages to `to`.
    fn carry(from: &mut App, to: &mut App) {
        for message in from
            .world_mut()
            .resource_mut::<PresenterLink>()
            .take_outgoing()
        {
            to.world_mut()
                .resource_mut::<PresenterLink>()
                .receive(message);
        }
    }

    #[test]
    fn presenter_view_asks_the_audience_page_which_tells_it_the_audience() {
        let mut presenter = app(LinkRole::Presenter);
        let mut audience = app(LinkRole::Audience);
        presenter
            .world_mut()
            .resource_mut::<PresenterLink>()
            .request(AudienceRequest::Kick { name: "Ana".into() });
        presenter.update();
        carry(&mut presenter, &mut audience);
        audience.update();
        let mut link = audience.world_mut().resource_mut::<PresenterLink>();
        assert!(link.take_peer_hello(), "Presenter View said hello");
        assert_eq!(
            link.take_audience_requests(),
            vec![AudienceRequest::Kick { name: "Ana".into() }]
        );

        let view = AudienceView {
            code: "ABC234".into(),
            players: 2,
            leaderboard: vec![("Ana".into(), 900)],
            download: true,
            ..Default::default()
        };
        link.send_audience(Some(view.clone()));
        carry(&mut audience, &mut presenter);
        presenter.update();
        assert_eq!(
            presenter.world().resource::<PresenterLink>().audience(),
            Some(&view)
        );
    }

    fn state(time: f64, playing: bool) -> LinkMessage {
        LinkMessage::State {
            state: LinkState {
                time,
                playing,
                blank: Blank::None,
            },
            heartbeat: false,
        }
    }

    #[test]
    fn a_local_seek_is_published_once() {
        let mut app = app(LinkRole::Audience);
        app.update();
        assert!(sent(&mut app).is_empty(), "nothing moved yet");
        app.world_mut().resource_mut::<Timeline>().current_time = 4.0;
        app.update();
        assert_eq!(sent(&mut app), vec![state(4.0, false)]);
        app.update();
        assert!(sent(&mut app).is_empty());
    }

    #[test]
    fn an_applied_state_is_not_echoed_back() {
        let mut app = app(LinkRole::Audience);
        app.update();
        let message = serde_json::to_string(&state(6.5, false)).unwrap();
        app.world_mut()
            .resource_mut::<PresenterLink>()
            .receive(message);
        app.update();
        assert_eq!(
            app.world().resource::<Timeline>().seek_request,
            Some(6.5),
            "the state is applied as a seek"
        );
        // The seek lands as the timeline's time, as the seek system does.
        {
            let mut timeline = app.world_mut().resource_mut::<Timeline>();
            timeline.current_time = 6.5;
            timeline.seek_request = None;
        }
        app.update();
        assert!(sent(&mut app).is_empty());
    }

    #[test]
    fn the_presenter_asks_for_the_state_and_the_audience_answers() {
        let mut presenter = app(LinkRole::Presenter);
        presenter.update();
        let hello = presenter
            .world_mut()
            .resource_mut::<PresenterLink>()
            .take_outgoing();
        assert_eq!(hello, vec![r#"{"type":"hello"}"#.to_string()]);

        let mut audience = app(LinkRole::Audience);
        audience.world_mut().resource_mut::<Timeline>().current_time = 3.0;
        audience.update();
        sent(&mut audience);
        for message in hello {
            audience
                .world_mut()
                .resource_mut::<PresenterLink>()
                .receive(message);
        }
        audience.update();
        assert_eq!(sent(&mut audience), vec![state(3.0, false)]);
    }

    #[test]
    fn blanking_and_play_are_published_but_playback_is_not() {
        let mut app = app(LinkRole::Audience);
        app.world_mut()
            .resource_mut::<gaanim_animation::DeltaTime>()
            .dt = 0.1;
        app.update();
        app.world_mut().resource_mut::<Timeline>().is_playing = true;
        app.update();
        assert_eq!(sent(&mut app), vec![state(0.0, true)]);
        // Playback advancing the playhead is not news.
        app.world_mut().resource_mut::<Timeline>().current_time = 0.1;
        app.update();
        assert!(sent(&mut app).is_empty());
        *app.world_mut().resource_mut::<AudienceBlank>() = AudienceBlank::Black;
        app.update();
        let messages = sent(&mut app);
        let [LinkMessage::State { state, .. }] = messages.as_slice() else {
            panic!("one state: {messages:?}");
        };
        assert_eq!(state.blank, Blank::Black);
    }

    #[test]
    fn heartbeats_only_correct_drift() {
        let mut app = app(LinkRole::Audience);
        app.world_mut().resource_mut::<Timeline>().current_time = 2.0;
        app.update();
        sent(&mut app);
        let beat = |time| {
            serde_json::to_string(&LinkMessage::State {
                state: LinkState {
                    time,
                    playing: false,
                    blank: Blank::None,
                },
                heartbeat: true,
            })
            .unwrap()
        };
        app.world_mut()
            .resource_mut::<PresenterLink>()
            .receive(beat(2.01));
        app.update();
        assert_eq!(app.world().resource::<Timeline>().seek_request, None);
        app.world_mut()
            .resource_mut::<PresenterLink>()
            .receive(beat(2.5));
        app.update();
        assert_eq!(app.world().resource::<Timeline>().seek_request, Some(2.5));
    }

    #[test]
    fn the_presenter_sends_heartbeats_while_playing() {
        let mut app = app(LinkRole::Presenter);
        app.world_mut()
            .resource_mut::<gaanim_animation::DeltaTime>()
            .dt = 0.6;
        app.world_mut().resource_mut::<Timeline>().is_playing = true;
        app.update();
        sent(&mut app);
        let mut beats = 0;
        for frame in 1..=4 {
            app.world_mut().resource_mut::<Timeline>().current_time = 0.6 * f64::from(frame);
            app.update();
            beats += sent(&mut app)
                .iter()
                .filter(|message| {
                    matches!(
                        message,
                        LinkMessage::State {
                            heartbeat: true,
                            ..
                        }
                    )
                })
                .count();
        }
        assert_eq!(beats, 2);
    }
}
