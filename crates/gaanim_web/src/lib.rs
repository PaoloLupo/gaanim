//! Gaanim web player.
//!
//! Runs the same playback application as `gaanim`, with the editor's playback bar
//! and Presenter View, compiled to WebAssembly. The page hands it a `.gaanim`
//! file picked or dropped whole ([`open_bundle`]), or one it downloads from
//! `?src=` in pieces ([`open_remote`], [`add_bytes`]): the player opens it
//! from the end of the file and asks the page for the byte ranges playback
//! needs (`window.gaanimFetch`) while the rest downloads. Bevy opens files
//! on its next frame.
//!
//! Presenter View runs as a second page, opened by this one: the same app
//! started with [`run`]`(true)`, which shows Presenter View alone. The two
//! pages keep the same playhead through `gaanim_editor::presenter_link`,
//! whose messages the pages carry over a `BroadcastChannel`.
//!
//! The audience page plays the bundle's audio through the browser: the
//! player hands the page each track and its file once
//! (`window.gaanimAudioTracks`, `window.gaanimAudioMedia`) and the playhead
//! every frame (`window.gaanimAudioState`); the page decodes and schedules
//! them with WebAudio.
//!
//! A presentation with audience polls talks to its relay from the audience
//! page: the page makes the requests and holds the results socket for
//! `gaanim_editor::web_relay` (`window.gaanimRelay*`), and hands back what it
//! hears ([`relay_response`], [`relay_socket_message`],
//! [`relay_socket_state`]).

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use bevy::prelude::*;
    use gaanim_bundle::{BundleError, BundleSource};
    use gaanim_editor::bundle_player::{self, BundlePlayback};
    use gaanim_editor::host::{HostOptions, WebPage, host_app};
    use gaanim_editor::presenter_link::{LinkRole, PresenterLink};
    use gaanim_editor::share_link::LinkTarget;
    use wasm_bindgen::prelude::*;

    /// Files picked or dropped whole, opened by [`open_requested_bundles`].
    static REQUESTS: Mutex<Vec<(String, Arc<[u8]>)>> = Mutex::new(Vec::new());
    /// The file downloading from a URL: its generation, name and bytes so
    /// far, until it opens.
    static REMOTE: Mutex<Option<(u32, String, BundleSource)>> = Mutex::new(None);
    /// The bytes of the file being played or opened, for [`add_bytes`].
    static SOURCE: Mutex<Option<(u32, BundleSource)>> = Mutex::new(None);
    /// Set when bytes arrive for a file that has not opened yet.
    static RETRY_OPEN: AtomicBool = AtomicBool::new(false);
    static GENERATION: AtomicU32 = AtomicU32::new(0);
    /// Where the page's link points, applied when the file opens.
    static LINK: Mutex<Option<LinkTarget>> = Mutex::new(None);
    /// The page's own Present button was clicked.
    static PRESENT: AtomicBool = AtomicBool::new(false);
    /// This page is Presenter View, which stays silent.
    static PRESENTER_PAGE: AtomicBool = AtomicBool::new(false);
    /// The volume the page remembered, to apply once.
    static VOLUME: Mutex<Option<gaanim_media::PreviewVolume>> = Mutex::new(None);
    /// Messages from the other page of the presentation.
    static LINK_INBOX: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// Frames the app has updated, for the page to measure the frame rate.
    static FRAMES: AtomicU32 = AtomicU32::new(0);

    fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Number of frames updated so far.
    #[wasm_bindgen(js_name = frameCount)]
    pub fn frame_count() -> u32 {
        FRAMES.load(Ordering::Relaxed)
    }

    fn count_frame() {
        FRAMES.fetch_add(1, Ordering::Relaxed);
    }

    #[wasm_bindgen]
    extern "C" {
        /// `window.gaanimStatus(kind, message)`, defined by the page.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimStatus)]
        fn status(kind: &str, message: &str);
        /// `window.gaanimOpenPresenter()`: open the Presenter View page.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimOpenPresenter)]
        fn open_presenter_page();
        /// `window.gaanimLinkSend(message)`: send to the other page.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimLinkSend)]
        fn link_send(message: &str);
        /// `window.gaanimFetch(ranges, urgent)`: download these byte ranges
        /// (start and end pairs) of the file; `urgent` when the frame on
        /// screen waits for them.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimFetch)]
        fn fetch_ranges(ranges: Vec<f64>, urgent: bool);
        /// `window.gaanimCopyLink(fragment)`: copy a link to the file.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimCopyLink)]
        fn copy_link(fragment: &str);
        /// `window.gaanimNotify(message)`: show a short message.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimNotify)]
        fn notify(message: &str);
        /// `window.gaanimToggleFullscreen()`: enter or leave full screen.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimToggleFullscreen)]
        fn toggle_fullscreen();
        /// `window.gaanimAudioTracks(json)`: the open file's audio tracks,
        /// replacing any earlier file's.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimAudioTracks)]
        fn audio_tracks(json: &str);
        /// `window.gaanimAudioMedia(entry, bytes)`: an audio file of the
        /// tracks, to decode.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimAudioMedia)]
        fn audio_media(entry: &str, bytes: Vec<u8>);
        /// `window.gaanimAudioState(time, playing, rate, level, muted)`: the
        /// playhead and the listener's volume.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimAudioState)]
        fn audio_state(time: f64, playing: bool, rate: f64, level: f32, muted: bool);
        /// `window.gaanimRelayKey(relay, code)`: the presenter key.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimRelayKey)]
        fn relay_key(relay: &str, code: &str) -> String;
        /// `window.gaanimRelayRequest(id, method, url, key, type, body)`.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimRelayRequest)]
        fn relay_request(
            id: u32,
            method: &str,
            url: &str,
            key: &str,
            content_type: &str,
            body: Vec<u8>,
        );
        /// `window.gaanimRelaySocket(url, key)`: keep the results socket open.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimRelaySocket)]
        fn relay_socket(url: &str, key: &str);
        /// `window.gaanimRelayClose()`: close it.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimRelayClose)]
        fn relay_close();
        /// `window.gaanimDownload(name, bytes)`: offer a file to save.
        #[wasm_bindgen(js_namespace = window, js_name = gaanimDownload)]
        fn download(name: &str, bytes: Vec<u8>);
    }

    /// The answer to relay request `id`; status 0 when it never arrived.
    #[wasm_bindgen(js_name = relayResponse)]
    pub fn relay_response(id: u32, status: u16, body: String) {
        gaanim_editor::web_relay::response(id, status, body);
    }

    /// A message the relay's results socket pushed.
    #[wasm_bindgen(js_name = relaySocketMessage)]
    pub fn relay_socket_message(text: String) {
        gaanim_editor::web_relay::socket_message(text);
    }

    /// The relay's results socket closed (`false`) or opened.
    #[wasm_bindgen(js_name = relaySocketState)]
    pub fn relay_socket_state(live: bool) {
        gaanim_editor::web_relay::socket_state(live);
    }

    /// A message from the other page of the presentation.
    #[wasm_bindgen(js_name = linkReceive)]
    pub fn link_receive(message: String) {
        lock(&LINK_INBOX).push(message);
    }

    fn receive_link_messages(mut link: ResMut<PresenterLink>) {
        for message in std::mem::take(&mut *lock(&LINK_INBOX)) {
            link.receive(message);
        }
    }

    fn send_link_messages(mut link: ResMut<PresenterLink>) {
        for message in link.take_outgoing() {
            link_send(&message);
        }
    }

    /// Queue a `.gaanim` file held whole for playback; `name` is its file
    /// name or URL.
    #[wasm_bindgen(js_name = openBundle)]
    pub fn open_bundle(name: String, bytes: Vec<u8>) {
        *lock(&REMOTE) = None;
        lock(&REQUESTS).push((name, bytes.into()));
    }

    /// Start opening a `.gaanim` file of `length` bytes that downloads in
    /// pieces, from the piece at its end. Returns the generation that
    /// [`add_bytes`] names its pieces with.
    #[wasm_bindgen(js_name = openRemote)]
    pub fn open_remote(name: String, length: f64, offset: f64, bytes: Vec<u8>) -> u32 {
        let generation = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
        let source = BundleSource::new(length as u64);
        source.insert(offset as u64, bytes.into());
        *lock(&SOURCE) = Some((generation, source.clone()));
        *lock(&REMOTE) = Some((generation, name, source));
        RETRY_OPEN.store(true, Ordering::Release);
        generation
    }

    /// Bytes of the file opened with [`open_remote`] as generation
    /// `generation`, starting at `offset`.
    #[wasm_bindgen(js_name = addBytes)]
    pub fn add_bytes(generation: u32, offset: f64, bytes: Vec<u8>) {
        if let Some((current, source)) = lock(&SOURCE).as_ref()
            && *current == generation
        {
            source.insert(offset as u64, bytes.into());
            RETRY_OPEN.store(true, Ordering::Release);
        }
    }

    /// The fragment of the page's URL: where to go once the file is open
    /// (see `gaanim_editor::share_link`).
    #[wasm_bindgen(js_name = setLink)]
    pub fn set_link(fragment: String) {
        *lock(&LINK) = LinkTarget::parse(&fragment);
    }

    /// The volume the page remembered from an earlier visit.
    #[wasm_bindgen(js_name = setVolume)]
    pub fn set_volume(level: f32, muted: bool) {
        *lock(&VOLUME) = Some(gaanim_media::PreviewVolume {
            level: level.clamp(0.0, 1.0),
            muted,
        });
    }

    fn apply_remembered_volume(mut volume: ResMut<gaanim_media::PreviewVolume>) {
        if let Some(remembered) = lock(&VOLUME).take() {
            *volume = remembered;
        }
    }

    /// Start presenting, from a click on the page's own Present button.
    #[wasm_bindgen]
    pub fn present() {
        PRESENT.store(true, Ordering::Release);
    }

    fn send_ranges(ranges: &[std::ops::Range<u64>], urgent: bool) {
        if ranges.is_empty() {
            return;
        }
        let flat = ranges
            .iter()
            .flat_map(|range| [range.start as f64, range.end as f64])
            .collect();
        fetch_ranges(flat, urgent);
    }

    /// What the viewer reads when a file cannot open.
    fn describe(error: &BundleError) -> String {
        match error {
            BundleError::UnsupportedVersion { found, .. } if *found > gaanim_bundle::VERSION => {
                "Este archivo usa una versión del formato .gaanim más nueva que este reproductor. \
                 Recarga la página para usar la última versión."
                    .into()
            }
            BundleError::UnsupportedVersion { generator, .. } => format!(
                "Este archivo lo grabó {generator} en un formato que ya no se lee. Vuelve a \
                 grabarlo con una versión reciente de Gaanim."
            ),
            BundleError::Corrupt(detail) => {
                format!("No es un archivo .gaanim o está dañado ({detail}).")
            }
            error => format!("No se pudo abrir el archivo: {error}."),
        }
    }

    /// Tell the page a file could not open: `damaged` when it is not a
    /// bundle or is damaged, which a partly downloaded file can also look
    /// like.
    fn failed(error: &BundleError) {
        let kind = if matches!(error, BundleError::Corrupt(_)) {
            "damaged"
        } else {
            "error"
        };
        status(kind, &describe(error));
    }

    /// Hand the page's link to the player once a file is open.
    fn apply_page_link(world: &mut World) {
        if world.contains_resource::<BundlePlayback>()
            && let Some(target) = lock(&LINK).take()
        {
            world.insert_resource(target);
        }
    }

    fn open_requested_bundles(world: &mut World) {
        let requests = std::mem::take(&mut *lock(&REQUESTS));
        for (name, bytes) in requests {
            let source = BundleSource::whole(bytes);
            *lock(&SOURCE) = None;
            match bundle_player::open_bundle_source(world, Path::new(&name), source) {
                Ok(()) => status("opened", &name),
                Err(error) => failed(&error),
            }
        }
        if !RETRY_OPEN.swap(false, Ordering::AcqRel) {
            return;
        }
        let Some((generation, name, source)) = lock(&REMOTE).clone() else {
            return;
        };
        match bundle_player::open_bundle_source(world, Path::new(&name), source) {
            Ok(()) => {
                let mut remote = lock(&REMOTE);
                if remote
                    .as_ref()
                    .is_some_and(|(current, ..)| *current == generation)
                {
                    *remote = None;
                }
                drop(remote);
                status("opened", &name);
            }
            Err(BundleError::Incomplete { missing }) => send_ranges(&missing, true),
            Err(error) => {
                *lock(&REMOTE) = None;
                failed(&error);
            }
        }
    }

    /// Ask the page for the bytes playback and Presenter View's previews
    /// need.
    fn request_wanted_bytes(playback: Option<ResMut<BundlePlayback>>) {
        if let Some(mut playback) = playback {
            let (wanted, urgent) = playback.take_wanted();
            send_ranges(&wanted, urgent);
        }
    }

    /// Audio files still to hand to the page, for the open file.
    #[derive(Resource, Default)]
    struct PendingAudio(Vec<String>);

    /// Hand the page the open file's audio and, every frame, the playhead.
    fn play_audio(
        playback: Option<ResMut<BundlePlayback>>,
        timeline: Option<Res<gaanim_timeline::timeline::Timeline>>,
        volume: Res<gaanim_media::PreviewVolume>,
        mut pending: ResMut<PendingAudio>,
    ) {
        if PRESENTER_PAGE.load(Ordering::Relaxed) {
            return;
        }
        let (Some(mut playback), Some(timeline)) = (playback, timeline) else {
            return;
        };
        if playback.is_added() {
            let tracks: Vec<serde_json::Value> = playback
                .audio()
                .iter()
                .map(|track| {
                    serde_json::json!({
                        "media": track.media,
                        "start": track.start_time,
                        "duration": track.duration,
                        "volume": track.volume,
                        "fadeIn": track.fade_in,
                        "fadeOut": track.fade_out,
                        "offset": track.source_offset,
                        "sourceDuration": track.source_duration,
                        "speed": track.speed,
                        "looping": track.looping,
                    })
                })
                .collect();
            audio_tracks(&serde_json::Value::Array(tracks).to_string());
            let mut entries: Vec<String> = playback
                .audio()
                .iter()
                .map(|track| track.media.clone())
                .collect();
            entries.sort();
            entries.dedup();
            pending.0 = entries;
        }
        let mut index = 0;
        while index < pending.0.len() {
            match playback.media(&pending.0[index]) {
                Ok(Some(bytes)) => {
                    audio_media(&pending.0[index], bytes);
                    pending.0.remove(index);
                }
                Ok(None) => index += 1,
                Err(error) => {
                    notify(&format!("No se pudo leer un audio del archivo: {error}."));
                    pending.0.remove(index);
                }
            }
        }
        audio_state(
            timeline.current_time,
            timeline.is_playing,
            timeline.playback_rate,
            volume.level,
            volume.muted,
        );
    }

    fn start_presenting_on_request(world: &mut World) {
        if PRESENT.swap(false, Ordering::AcqRel) {
            gaanim_editor::start_presenting(world);
        }
    }

    /// Start the player; `presenter` starts the Presenter View page.
    #[wasm_bindgen]
    pub fn run(presenter: bool) {
        console_error_panic_hook::set_once();
        PRESENTER_PAGE.store(presenter, Ordering::Relaxed);
        let _ = gaanim_editor::web_relay::HOOKS.set(gaanim_editor::web_relay::WebRelayHooks {
            key: relay_key,
            request: relay_request,
            open_socket: relay_socket,
            close_socket: relay_close,
            download,
            notify,
        });
        let mut app = host_app(&HostOptions {
            presenter_page: presenter,
            ..Default::default()
        });
        app.insert_resource(PresenterLink::new(if presenter {
            LinkRole::Presenter
        } else {
            LinkRole::Audience
        }))
        .insert_resource(WebPage {
            open_presenter: open_presenter_page,
            copy_link,
            notify,
            toggle_fullscreen,
        })
        .init_resource::<PendingAudio>()
        .add_systems(
            PreUpdate,
            (
                (open_requested_bundles, apply_page_link).chain(),
                receive_link_messages,
                start_presenting_on_request,
                apply_remembered_volume,
            ),
        )
        .add_systems(
            Last,
            (
                send_link_messages,
                (play_audio, request_wanted_bytes).chain(),
                count_frame,
            ),
        );
        status("ready", "");
        app.run();
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::open_bundle;
