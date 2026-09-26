//! ALSA prints its configuration errors straight to stderr (`ALSA lib
//! confmisc.c:855:(parse_card) cannot find card '0'`) on machines without a
//! sound card, such as WSL, servers and CI. Audio already degrades gracefully
//! there, so those lines only bury Gaanim's own output.

use std::ffi::{c_char, c_int, c_void};

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
}

unsafe extern "C" fn ignore_alsa_error(
    _file: *const c_char,
    _line: c_int,
    _function: *const c_char,
    _error: c_int,
    _format: *const c_char,
    _arguments: *mut c_void,
) {
}

/// Silences ALSA's error messages on the calling thread, which must be the one
/// that opens audio devices (Bevy opens its audio output on the main thread).
pub fn silence_alsa_errors() {
    // SAFETY: installs a handler with the exact signature ALSA expects; the
    // handler ignores its arguments and never unwinds.
    unsafe {
        snd_lib_error_set_local(Some(ignore_alsa_error));
    }
}
