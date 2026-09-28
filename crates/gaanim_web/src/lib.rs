//! Gaanim web player.
//!
//! Runs the same application as `gaanim-play`, with the editor's playback bar
//! and Presenter View, compiled to WebAssembly. The page hands it the bytes of
//! a `.gaanim` file (picked, dropped or fetched from `?src=`) through
//! [`open_bundle`]; Bevy opens them on its next frame.
//!
//! Presenter View runs as a second page, opened by this one: the same app
//! started with [`run`]`(true)`, which shows Presenter View alone. The two
//! pages keep the same playhead through `gaanim_editor::presenter_link`,
//! whose messages the pages carry over a `BroadcastChannel`.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use bevy::prelude::*;
    use gaanim_editor::bundle_player;
    use gaanim_editor::host::{HostOptions, WebPage, host_app};
    use gaanim_editor::presenter_link::{LinkRole, PresenterLink};
    use wasm_bindgen::prelude::*;

    /// Bundles handed over by the page, opened by [`open_requested_bundles`].
    static REQUESTS: Mutex<Vec<(String, Arc<[u8]>)>> = Mutex::new(Vec::new());
    /// Messages from the other page of the presentation.
    static LINK_INBOX: Mutex<Vec<String>> = Mutex::new(Vec::new());
    /// Frames the app has updated, for the page to measure the frame rate.
    static FRAMES: AtomicU32 = AtomicU32::new(0);

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
    }

    /// A message from the other page of the presentation.
    #[wasm_bindgen(js_name = linkReceive)]
    pub fn link_receive(message: String) {
        LINK_INBOX
            .lock()
            .expect("link inbox poisoned")
            .push(message);
    }

    fn receive_link_messages(mut link: ResMut<PresenterLink>) {
        for message in std::mem::take(&mut *LINK_INBOX.lock().expect("link inbox poisoned")) {
            link.receive(message);
        }
    }

    fn send_link_messages(mut link: ResMut<PresenterLink>) {
        for message in link.take_outgoing() {
            link_send(&message);
        }
    }

    /// Queue a `.gaanim` file for playback; `name` is its file name or URL.
    #[wasm_bindgen(js_name = openBundle)]
    pub fn open_bundle(name: String, bytes: Vec<u8>) {
        REQUESTS
            .lock()
            .expect("bundle requests poisoned")
            .push((name, bytes.into()));
    }

    fn open_requested_bundles(world: &mut World) {
        let requests = std::mem::take(&mut *REQUESTS.lock().expect("bundle requests poisoned"));
        for (name, bytes) in requests {
            match bundle_player::open_bundle_bytes(world, Path::new(&name), bytes) {
                Ok(()) => status("opened", &name),
                Err(error) => status("error", &error),
            }
        }
    }

    /// Start the player; `presenter` starts the Presenter View page.
    #[wasm_bindgen]
    pub fn run(presenter: bool) {
        console_error_panic_hook::set_once();
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
        })
        .add_systems(PreUpdate, (open_requested_bundles, receive_link_messages))
        .add_systems(Last, (send_link_messages, count_frame));
        status("ready", "");
        app.run();
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::open_bundle;
