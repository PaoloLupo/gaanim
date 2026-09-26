//! ALSA prints its errors straight to stderr, several lines per problem (a
//! machine without a sound card produces eight `ALSA lib confmisc.c:855:...`
//! lines at startup). Route them through Gaanim's status lines instead: the
//! first message of each burst is shown as an `audio` warning, and the cascade
//! that follows it within a second is dropped.

use std::ffi::{CStr, c_char, c_int, c_void};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

type LocalErrorHandler = unsafe extern "C" fn(
    file: *const c_char,
    line: c_int,
    function: *const c_char,
    error: c_int,
    format: *const c_char,
    arguments: *mut c_void,
);

unsafe extern "C" {
    // libasound is linked through cpal. Unlike the global handler, the local
    // one is not variadic (it receives a `va_list`), so it can be written in
    // Rust.
    fn snd_lib_error_set_local(handler: Option<LocalErrorHandler>) -> Option<LocalErrorHandler>;
    fn snd_strerror(error: c_int) -> *const c_char;
    // On x86-64 and AArch64 Linux a `va_list` parameter travels as a pointer,
    // so the one ALSA handed over can be forwarded as is.
    fn vsnprintf(
        buffer: *mut c_char,
        size: usize,
        format: *const c_char,
        arguments: *mut c_void,
    ) -> c_int;
}

/// Milliseconds since the epoch when the last ALSA message was shown.
static LAST_SHOWN_MS: AtomicU64 = AtomicU64::new(0);
const BURST_MS: u64 = 1_000;

unsafe extern "C" fn report_alsa_error(
    _file: *const c_char,
    _line: c_int,
    _function: *const c_char,
    error: c_int,
    format: *const c_char,
    arguments: *mut c_void,
) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64);
    let last = LAST_SHOWN_MS.swap(now, Ordering::Relaxed);
    if now.saturating_sub(last) < BURST_MS {
        return;
    }
    let mut buffer = [0 as c_char; 512];
    // SAFETY: ALSA passes a valid printf format with its matching `va_list`;
    // vsnprintf writes at most `buffer.len()` bytes, NUL-terminated.
    let written = unsafe {
        if format.is_null() {
            -1
        } else {
            vsnprintf(buffer.as_mut_ptr(), buffer.len(), format, arguments)
        }
    };
    let mut message = if written > 0 {
        // SAFETY: vsnprintf NUL-terminated the buffer.
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    };
    if error != 0 {
        // SAFETY: snd_strerror returns a static string for any code.
        let reason = unsafe { CStr::from_ptr(snd_strerror(error)) }.to_string_lossy();
        message = if message.is_empty() {
            reason.into_owned()
        } else {
            format!("{message} ({reason})")
        };
    }
    gaanim_core::console::warn(
        "audio",
        format!("ALSA: {message} · preview audio may be off"),
    );
}

/// Routes ALSA's error messages on the calling thread, which must be the one
/// that opens audio devices (Bevy opens its audio output on the main thread).
pub fn route_alsa_errors() {
    if !cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
        return;
    }
    // SAFETY: installs a handler with the exact signature ALSA expects; it
    // never unwinds (formatting and writing to stderr cannot panic here).
    unsafe {
        snd_lib_error_set_local(Some(report_alsa_error));
    }
}
