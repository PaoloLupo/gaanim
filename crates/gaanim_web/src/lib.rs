//! Gaanim web player.
//!
//! Runs the same application as `gaanim-play`, with the editor's playback bar
//! and Presenter View, compiled to WebAssembly. The page hands it the bytes of
//! a `.gaanim` file (picked, dropped or fetched from `?src=`) through
//! [`open_bundle`]; Bevy opens them on its next frame.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::Path;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::{Arc, Mutex};

    use bevy::prelude::*;
    use gaanim_editor::bundle_player;
    use gaanim_editor::host::{HostOptions, host_app};
    use wasm_bindgen::prelude::*;

    /// Bundles handed over by the page, opened by [`open_requested_bundles`].
    static REQUESTS: Mutex<Vec<(String, Arc<[u8]>)>> = Mutex::new(Vec::new());
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

    #[wasm_bindgen(start)]
    pub fn start() {
        console_error_panic_hook::set_once();
        let mut app = host_app(&HostOptions::default());
        app.add_systems(PreUpdate, open_requested_bundles)
            .add_systems(Last, count_frame);
        status("ready", "");
        app.run();
    }
}

#[cfg(target_arch = "wasm32")]
pub use web::open_bundle;
