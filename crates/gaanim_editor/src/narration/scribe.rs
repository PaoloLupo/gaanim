//! Dictating the talk's script with Zoom's Scribe Live.
//!
//! While the speaker presents the scene, the microphone streams to Scribe
//! Live as 16 kHz mono PCM16, and each speech turn Scribe transcribes comes
//! back with the instant it started on the recording clock. The narration
//! panel files every turn under the slide part being said then and writes
//! the turns into the script.
//!
//! Privacy: the token is typed for the session, kept in the system's
//! credential store (Windows Credential Manager, encrypted for the user;
//! see [`vault`]) or read from `GAANIM_SCRIBE_TOKEN`. None of them is in
//! the project, so a repository never holds it. It is never logged or
//! shown, and it only travels in the `Authorization` header over TLS.
//! Scribe receives the audio and the session settings: the language and,
//! only when the speaker opts in, the names of the slides and stops as
//! vocabulary.
//!
//! Cost: Scribe Live bills the audio it receives. One session runs at a
//! time and never reconnects by itself, and the streaming thread ends it
//! on its own after the session's minute limit, after two minutes without
//! speech, or once the microphone stops delivering audio, whether or not
//! the editor is drawing frames.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::time::Duration;

/// Environment variable read for the token.
pub(crate) const TOKEN_VARIABLE: &str = "GAANIM_SCRIBE_TOKEN";
#[cfg(not(target_arch = "wasm32"))]
const LIVE_URL: &str = "wss://api.zoom.us/v2/aiservices/scribe/live";
#[cfg(not(target_arch = "wasm32"))]
const SUBPROTOCOL: &str = "live-asr";
/// Scribe takes 16 kHz audio.
const SAMPLE_RATE: u32 = 16_000;
/// About 100 ms of audio per frame, as Scribe recommends.
const FRAME_SAMPLES: usize = 1_600;
/// Scribe Live closes sessions after two hours.
pub(crate) const MAX_SESSION_MINUTES: u32 = 120;
/// Without speech for this long, the session ends.
const SILENCE_LIMIT: Duration = Duration::from_secs(120);
/// Without new audio from the microphone for this long, the session ends.
const NO_AUDIO_LIMIT: Duration = Duration::from_secs(3);
/// How long to wait for the last turns after asking Scribe to close.
const CLOSE_WAIT: Duration = Duration::from_secs(10);

/// Locales Scribe transcribes.
pub(crate) const LANGUAGES: [&str; 11] = [
    "es-ES", "en-US", "pt-BR", "pt-PT", "fr-FR", "de-DE", "it-IT", "ja-JP", "zh-CN", "ar-SA",
    "ar-AE",
];

/// The Scribe locale for a BCP 47 `tag`: the same one (`es-ES`), else the
/// first of its language (`es` and `es-PE` are `es-ES`, `pt` is `pt-BR`).
pub(crate) fn locale_for(tag: &str) -> Option<&'static str> {
    let tag = tag.trim().replace('_', "-");
    LANGUAGES
        .iter()
        .find(|locale| locale.eq_ignore_ascii_case(&tag))
        .or_else(|| {
            let language = tag.split('-').next()?;
            LANGUAGES.iter().find(|locale| {
                locale
                    .split('-')
                    .next()
                    .is_some_and(|own| own.eq_ignore_ascii_case(language))
            })
        })
        .copied()
}

/// Whether a session is streaming, so a second one never starts.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// A Scribe token. It never prints.
pub(crate) struct ScribeToken(String);

impl fmt::Debug for ScribeToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ScribeToken(hidden)")
    }
}

/// Where the token came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenSource {
    /// Typed in the panel for this session.
    Typed,
    /// The system's credential store.
    Vault,
    /// `GAANIM_SCRIBE_TOKEN`.
    Environment,
}

impl ScribeToken {
    /// The token in `text`, without a `Bearer ` prefix, unless it is empty.
    pub(crate) fn new(text: &str) -> Option<Self> {
        let text = text.trim();
        let text = text
            .strip_prefix("Bearer ")
            .or_else(|| text.strip_prefix("bearer "))
            .unwrap_or(text)
            .trim();
        (!text.is_empty()).then(|| Self(text.to_owned()))
    }

    /// The token typed for this session, else the `stored` one, else the
    /// one in `environment`'s `GAANIM_SCRIBE_TOKEN`.
    pub(crate) fn resolve(
        typed: &str,
        stored: impl FnOnce() -> Option<Self>,
        environment: impl Fn(&str) -> Option<String>,
    ) -> Option<(Self, TokenSource)> {
        Self::new(typed)
            .map(|token| (token, TokenSource::Typed))
            .or_else(|| stored().map(|token| (token, TokenSource::Vault)))
            .or_else(|| {
                environment(TOKEN_VARIABLE)
                    .and_then(|text| Self::new(&text))
                    .map(|token| (token, TokenSource::Environment))
            })
    }
}

/// The token kept in the system's credential store, encrypted for this
/// user and outside every project: Windows Credential Manager, under
/// `gaanim/scribe-token`. Other systems keep none.
pub(crate) mod vault {
    use super::ScribeToken;

    /// Whether this system has a credential store Gaanim uses.
    pub(crate) const AVAILABLE: bool = cfg!(windows);
    #[cfg(windows)]
    const TARGET: &str = "gaanim/scribe-token";
    /// Windows keeps at most this many bytes per credential.
    #[cfg(windows)]
    const MAX_BYTES: usize = 2_560;

    /// The stored token, if any.
    pub(crate) fn load() -> Option<ScribeToken> {
        #[cfg(windows)]
        {
            let bytes = credentials::read(TARGET)?;
            ScribeToken::new(&String::from_utf8(bytes).ok()?)
        }
        #[cfg(not(windows))]
        {
            None
        }
    }

    /// Keep `token`, replacing the stored one.
    pub(crate) fn save(token: &ScribeToken) -> Result<(), String> {
        #[cfg(windows)]
        {
            if token.0.len() > MAX_BYTES {
                return Err(format!(
                    "el token pasa de {MAX_BYTES} bytes, lo que Windows guarda por credencial"
                ));
            }
            credentials::write(
                TARGET,
                "Token de Scribe de Zoom para Gaanim",
                token.0.as_bytes(),
            )
        }
        #[cfg(not(windows))]
        {
            let _ = token;
            Err("este sistema no tiene un almacén de credenciales que Gaanim use".to_owned())
        }
    }

    /// Remove the stored token; nothing stored is not an error.
    pub(crate) fn forget() -> Result<(), String> {
        #[cfg(windows)]
        {
            credentials::delete(TARGET)
        }
        #[cfg(not(windows))]
        {
            Ok(())
        }
    }

    /// The four Credential Manager calls Gaanim needs, declared here rather
    /// than through `windows-sys`, whose extra feature would rebuild Bevy.
    #[cfg(windows)]
    mod credentials {
        use std::ffi::c_void;
        use std::ptr::null_mut;

        #[repr(C)]
        struct FileTime {
            low: u32,
            high: u32,
        }

        /// `CREDENTIALW`.
        #[repr(C)]
        struct Credential {
            flags: u32,
            kind: u32,
            target_name: *mut u16,
            comment: *mut u16,
            last_written: FileTime,
            blob_size: u32,
            blob: *mut u8,
            persist: u32,
            attribute_count: u32,
            attributes: *mut c_void,
            target_alias: *mut u16,
            user_name: *mut u16,
        }

        const CRED_TYPE_GENERIC: u32 = 1;
        /// Kept for this user on this machine, not roamed to others.
        const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;
        const ERROR_NOT_FOUND: i32 = 1168;

        #[link(name = "advapi32")]
        unsafe extern "system" {
            fn CredWriteW(credential: *const Credential, flags: u32) -> i32;
            fn CredReadW(
                target: *const u16,
                kind: u32,
                flags: u32,
                credential: *mut *mut Credential,
            ) -> i32;
            fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
            fn CredFree(buffer: *const c_void);
        }

        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain(std::iter::once(0)).collect()
        }

        pub(super) fn read(target: &str) -> Option<Vec<u8>> {
            let target = wide(target);
            let mut credential: *mut Credential = null_mut();
            // SAFETY: `target` is NUL-terminated and `credential` is a valid
            // out pointer; on success Windows allocates the credential, which
            // is freed below with CredFree.
            let found =
                unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } != 0;
            if !found || credential.is_null() {
                return None;
            }
            // SAFETY: CredReadW succeeded, so `credential` points at a
            // CREDENTIALW whose blob holds `blob_size` bytes.
            let bytes = unsafe {
                let stored = &*credential;
                if stored.blob.is_null() {
                    Vec::new()
                } else {
                    std::slice::from_raw_parts(stored.blob, stored.blob_size as usize).to_vec()
                }
            };
            // SAFETY: `credential` came from CredReadW and is freed once.
            unsafe { CredFree(credential.cast::<c_void>()) };
            Some(bytes)
        }

        pub(super) fn write(target: &str, comment: &str, secret: &[u8]) -> Result<(), String> {
            let mut target = wide(target);
            let mut comment = wide(comment);
            let mut blob = secret.to_vec();
            let credential = Credential {
                flags: 0,
                kind: CRED_TYPE_GENERIC,
                target_name: target.as_mut_ptr(),
                comment: comment.as_mut_ptr(),
                last_written: FileTime { low: 0, high: 0 },
                blob_size: blob.len() as u32,
                blob: blob.as_mut_ptr(),
                persist: CRED_PERSIST_LOCAL_MACHINE,
                attribute_count: 0,
                attributes: null_mut(),
                target_alias: null_mut(),
                user_name: null_mut(),
            };
            // SAFETY: every pointer in `credential` points into a buffer
            // that outlives the call, and CredWriteW copies what it keeps.
            let written = unsafe { CredWriteW(&credential, 0) } != 0;
            let error = std::io::Error::last_os_error();
            // Leave no copy of the secret in this buffer.
            blob.fill(0);
            if written {
                Ok(())
            } else {
                Err(format!("Windows no guardó la credencial: {error}"))
            }
        }

        pub(super) fn delete(target: &str) -> Result<(), String> {
            let target = wide(target);
            // SAFETY: `target` is NUL-terminated.
            if unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) } != 0 {
                return Ok(());
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NOT_FOUND) {
                Ok(())
            } else {
                Err(format!("Windows no borró la credencial: {error}"))
            }
        }

        #[cfg(test)]
        mod tests {
            /// Touches the real Credential Manager under a throwaway name:
            /// run it with `--ignored`.
            #[test]
            #[ignore]
            fn credentials_round_trip_through_the_credential_manager() {
                let target = format!("gaanim/test-{}", std::process::id());
                assert_eq!(super::read(&target), None);
                super::write(&target, "prueba", b"secreto").unwrap();
                assert_eq!(super::read(&target).as_deref(), Some(&b"secreto"[..]));
                super::write(&target, "prueba", b"otro").unwrap();
                assert_eq!(super::read(&target).as_deref(), Some(&b"otro"[..]));
                super::delete(&target).unwrap();
                assert_eq!(super::read(&target), None);
                super::delete(&target).unwrap();
            }
        }
    }
}

/// What a session asks Scribe for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ScribeConfig {
    /// A locale of [`LANGUAGES`].
    pub(crate) language: String,
    /// Words Scribe should expect, sent only when the speaker opts in.
    pub(crate) phrases: Vec<String>,
    /// The session ends after this many minutes of audio.
    pub(crate) max_minutes: u32,
}

/// The `session.update` event that configures a session. It names the
/// audio format both ways Zoom's references spell it.
pub(crate) fn session_update(config: &ScribeConfig) -> String {
    let mut event = serde_json::json!({
        "type": "session.update",
        "language": config.language,
        "input_audio_format": "pcm16",
        "audio": { "format": "pcm16" },
    });
    if !config.phrases.is_empty() {
        event["transcription"] = serde_json::json!({ "phrase_list": config.phrases });
    }
    event.to_string()
}

/// Why a session ended before the speaker finished it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    /// The session's minute limit.
    Limit,
    /// Two minutes without speech.
    Silence,
    /// The microphone stopped delivering audio.
    NoAudio,
}

/// What a session reports.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ScribeEvent {
    /// Scribe accepted the session.
    Ready,
    /// The speaker started (`true`) or stopped talking.
    Speech(bool),
    /// A transcribed turn, in seconds of the recording.
    Turn { start: f64, end: f64, text: String },
    /// A problem Scribe reported that does not end the session.
    Warning(String),
    /// The session ended by itself; turns may still arrive until `Closed`.
    Stopping(StopReason),
    /// The session failed and ended.
    Failed(String),
    /// The session ended.
    Closed,
}

/// The event a text frame from Scribe stands for, if any.
pub(crate) fn parse_event(text: &str) -> Option<ScribeEvent> {
    let event: serde_json::Value = serde_json::from_str(text).ok()?;
    let millis = |field: &str| event.get(field).and_then(serde_json::Value::as_f64);
    match event.get("type")?.as_str()? {
        "session.created" | "session.updated" => Some(ScribeEvent::Ready),
        "input_audio_buffer.speech_started" => Some(ScribeEvent::Speech(true)),
        "input_audio_buffer.speech_stopped" => Some(ScribeEvent::Speech(false)),
        "transcription.completed" => {
            let text = event.get("transcript")?.as_str()?.trim();
            if text.is_empty() {
                return None;
            }
            let start = millis("audio_start_ms")? / 1000.0;
            let end = millis("audio_end_ms").map_or(start, |end| end / 1000.0);
            Some(ScribeEvent::Turn {
                start,
                end,
                text: text.to_owned(),
            })
        }
        "error" => {
            let error = event.get("error");
            let message = error
                .and_then(|error| error.get("message"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Scribe informó un error")
                .to_owned();
            let fatal = error
                .and_then(|error| error.get("fatal"))
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true);
            Some(if fatal {
                ScribeEvent::Failed(message)
            } else {
                ScribeEvent::Warning(message)
            })
        }
        "session.closed" => Some(ScribeEvent::Closed),
        _ => None,
    }
}

/// What an HTTP status in the handshake means, without echoing anything
/// that was sent.
#[cfg(not(target_arch = "wasm32"))]
fn describe_status(status: u16) -> String {
    match status {
        401 => "Zoom rechazó el token (401): puede haber caducado".to_owned(),
        403 => "el token no tiene permiso para Scribe Live (403)".to_owned(),
        426 => "Zoom pidió otro protocolo de conexión (426)".to_owned(),
        429 => "Zoom limita las sesiones de Scribe ahora mismo (429); prueba más tarde".to_owned(),
        503 => "Scribe no está disponible ahora (503)".to_owned(),
        status => format!("Zoom respondió {status} al conectar"),
    }
}

/// Mono audio for Scribe.
pub(crate) trait AudioSource: Send + 'static {
    fn sample_rate(&self) -> u32;
    /// The samples captured since the previous call.
    fn take(&mut self) -> Vec<f32>;
}

/// Turns mono samples at any rate into 16 kHz PCM16: each output sample is
/// the mean of the input samples it covers, a box filter against aliasing.
pub(crate) struct Downsampler {
    /// Input samples per output sample.
    ratio: f64,
    pending: Vec<f32>,
    /// Absolute index of `pending[0]` in the input.
    consumed: u64,
    produced: u64,
}

impl Downsampler {
    pub(crate) fn new(sample_rate: u32) -> Self {
        Self {
            ratio: sample_rate.max(1) as f64 / SAMPLE_RATE as f64,
            pending: Vec::new(),
            consumed: 0,
            produced: 0,
        }
    }

    pub(crate) fn push(&mut self, samples: &[f32]) -> Vec<i16> {
        self.pending.extend_from_slice(samples);
        let available = self.consumed + self.pending.len() as u64;
        let mut output = Vec::new();
        loop {
            let start = (self.produced as f64 * self.ratio).floor() as u64;
            let end = (((self.produced + 1) as f64 * self.ratio).floor() as u64).max(start + 1);
            if end > available {
                break;
            }
            let window =
                &self.pending[(start - self.consumed) as usize..(end - self.consumed) as usize];
            let mean = window.iter().sum::<f32>() / window.len() as f32;
            output.push((mean.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16);
            self.produced += 1;
        }
        let next = ((self.produced as f64 * self.ratio).floor() as u64).min(available);
        self.pending.drain(..(next - self.consumed) as usize);
        self.consumed = next;
        output
    }
}

/// The text of each slide part, in the order first heard: a turn joins the
/// part being said when it started. `positions` holds the recording
/// seconds at which the part changed, with the slide (by name) and part
/// from then on, or `None` outside any slide.
pub(crate) fn dictated_parts(
    positions: &[(f64, Option<(String, usize)>)],
    turns: &[(f64, String)],
) -> Vec<(String, usize, String)> {
    let mut parts: Vec<(String, usize, String)> = Vec::new();
    for (start, text) in turns {
        // A turn that starts a breath before the advance belongs after it.
        let Some(Some((slide, part))) = positions
            .iter()
            .rev()
            .find(|(time, _)| *time <= start + 0.05)
            .map(|(_, position)| position)
        else {
            continue;
        };
        match parts
            .iter_mut()
            .find(|(name, index, _)| name == slide && index == part)
        {
            Some((_, _, joined)) => {
                joined.push(' ');
                joined.push_str(text);
            }
            None => parts.push((slide.clone(), *part, text.clone())),
        }
    }
    parts
}

const RUNNING: u8 = 0;
const FINISHING: u8 = 1;
const ABORTING: u8 = 2;

/// A Scribe Live session streaming in the background. Dropping it ends
/// the session at once.
pub(crate) struct ScribeSession {
    events: crossbeam_channel::Receiver<ScribeEvent>,
    control: Arc<AtomicU8>,
    /// 16 kHz samples sent so far: the audio Scribe bills. The streaming
    /// thread enforces the limit with its own handle; tests read this one.
    #[cfg_attr(not(test), allow(dead_code))]
    streamed: Arc<AtomicU64>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl ScribeSession {
    /// Open a session that streams `source` to Scribe Live. Fails when
    /// another session is streaming.
    pub(crate) fn start(
        token: ScribeToken,
        config: ScribeConfig,
        source: impl AudioSource,
    ) -> Result<Self, String> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::start_at(LIVE_URL, token, config, source)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (token, config, source);
            Err("Scribe solo está disponible en la aplicación de escritorio".to_owned())
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn start_at(
        url: &str,
        token: ScribeToken,
        config: ScribeConfig,
        source: impl AudioSource,
    ) -> Result<Self, String> {
        if ACTIVE
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("ya hay una sesión de Scribe en curso".to_owned());
        }
        let (events_tx, events) = crossbeam_channel::unbounded();
        let control = Arc::new(AtomicU8::new(RUNNING));
        let streamed = Arc::new(AtomicU64::new(0));
        let thread = {
            let url = url.to_owned();
            let control = control.clone();
            let streamed = streamed.clone();
            std::thread::Builder::new()
                .name("gaanim-scribe".into())
                .spawn(move || {
                    stream(
                        &url, token, &config, source, &events_tx, &control, &streamed,
                    );
                    ACTIVE.store(false, Ordering::SeqCst);
                })
        };
        match thread {
            Ok(thread) => Ok(Self {
                events,
                control,
                streamed,
                thread: Some(thread),
            }),
            Err(error) => {
                ACTIVE.store(false, Ordering::SeqCst);
                Err(error.to_string())
            }
        }
    }

    /// The events reported since the previous call.
    pub(crate) fn poll(&self) -> Vec<ScribeEvent> {
        self.events.try_iter().collect()
    }

    /// Send the audio still pending and ask Scribe to close; the last turns
    /// arrive before `Closed`.
    pub(crate) fn finish(&self) {
        let _ =
            self.control
                .compare_exchange(RUNNING, FINISHING, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Seconds of audio sent to Scribe, which is what it bills.
    #[cfg(test)]
    fn streamed_seconds(&self) -> f64 {
        self.streamed.load(Ordering::Relaxed) as f64 / SAMPLE_RATE as f64
    }
}

impl Drop for ScribeSession {
    fn drop(&mut self) {
        self.control.store(ABORTING, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
type Socket = tungstenite::WebSocket<tungstenite::stream::MaybeTlsStream<std::net::TcpStream>>;

#[cfg(not(target_arch = "wasm32"))]
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Reads wake this often, to keep the audio flowing between server events.
#[cfg(not(target_arch = "wasm32"))]
const READ_TICK: Duration = Duration::from_millis(20);

/// Open the socket: TCP with a timeout, TLS for `wss`, and the handshake
/// with the token and the `live-asr` subprotocol. Errors never quote the
/// request.
#[cfg(not(target_arch = "wasm32"))]
fn connect(url: &str, token: &ScribeToken) -> Result<Socket, String> {
    use std::net::{TcpStream, ToSocketAddrs};
    use tungstenite::client::IntoClientRequest;
    use tungstenite::http::{HeaderValue, header};

    let mut request = url
        .into_client_request()
        .map_err(|_| "la dirección de Scribe no es válida".to_owned())?;
    let mut authorization = HeaderValue::from_str(&format!("Bearer {}", token.0))
        .map_err(|_| "el token tiene caracteres no válidos".to_owned())?;
    authorization.set_sensitive(true);
    request
        .headers_mut()
        .insert(header::AUTHORIZATION, authorization);
    request.headers_mut().insert(
        header::SEC_WEBSOCKET_PROTOCOL,
        HeaderValue::from_static(SUBPROTOCOL),
    );
    let host = request
        .uri()
        .host()
        .ok_or_else(|| "la dirección de Scribe no tiene host".to_owned())?
        .to_owned();
    let secure = request.uri().scheme_str() == Some("wss");
    let port = request
        .uri()
        .port_u16()
        .unwrap_or(if secure { 443 } else { 80 });
    let unreachable = |error: &dyn fmt::Display| format!("no se pudo conectar con Scribe: {error}");
    let addresses = (host.as_str(), port)
        .to_socket_addrs()
        .map_err(|error| unreachable(&error))?;
    let mut last = None;
    let mut stream = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(connected) => {
                stream = Some(connected);
                break;
            }
            Err(error) => last = Some(error),
        }
    }
    let stream = match (stream, last) {
        (Some(stream), _) => stream,
        (None, Some(error)) => return Err(unreachable(&error)),
        (None, None) => return Err(unreachable(&"la dirección no resuelve")),
    };
    stream
        .set_read_timeout(Some(CONNECT_TIMEOUT))
        .and_then(|()| stream.set_write_timeout(Some(CONNECT_TIMEOUT)))
        .map_err(|error| unreachable(&error))?;
    let connector = if secure {
        tungstenite::Connector::NativeTls(
            native_tls::TlsConnector::new().map_err(|error| unreachable(&error))?,
        )
    } else {
        tungstenite::Connector::Plain
    };
    let (socket, _) = tungstenite::client_tls_with_config(request, stream, None, Some(connector))
        .map_err(|error| match error {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)) => {
            describe_status(response.status().as_u16())
        }
        tungstenite::HandshakeError::Failure(error) => unreachable(&error),
        tungstenite::HandshakeError::Interrupted(_) => unreachable(&"la conexión tardó demasiado"),
    })?;
    let tcp = match socket.get_ref() {
        tungstenite::stream::MaybeTlsStream::Plain(tcp) => Some(tcp),
        tungstenite::stream::MaybeTlsStream::NativeTls(tls) => Some(tls.get_ref()),
        _ => None,
    };
    if let Some(tcp) = tcp {
        tcp.set_read_timeout(Some(READ_TICK))
            .map_err(|error| unreachable(&error))?;
    }
    Ok(socket)
}

/// Stream `source` to Scribe until the session finishes, fails or is
/// aborted, reporting what happens through `events`. It never reconnects.
#[cfg(not(target_arch = "wasm32"))]
fn stream(
    url: &str,
    token: ScribeToken,
    config: &ScribeConfig,
    mut source: impl AudioSource,
    events: &crossbeam_channel::Sender<ScribeEvent>,
    control: &AtomicU8,
    streamed: &AtomicU64,
) {
    use std::time::Instant;
    use tungstenite::Message;

    let connected = connect(url, &token);
    // The handshake is the only use of the token.
    drop(token);
    let mut socket = match connected {
        Ok(socket) => socket,
        Err(error) => {
            let _ = events.send(ScribeEvent::Failed(error));
            let _ = events.send(ScribeEvent::Closed);
            return;
        }
    };
    let fail = |socket: &mut Socket, message: String| {
        let _ = events.send(ScribeEvent::Failed(message));
        let _ = events.send(ScribeEvent::Closed);
        let _ = socket.close(None);
        let _ = socket.flush();
    };
    if socket.send(Message::text(session_update(config))).is_err() {
        fail(&mut socket, "se cortó la conexión con Scribe".to_owned());
        return;
    }
    let max_samples =
        u64::from(config.max_minutes.clamp(1, MAX_SESSION_MINUTES)) * 60 * u64::from(SAMPLE_RATE);
    let mut downsampler = Downsampler::new(source.sample_rate());
    let mut pending: Vec<i16> = Vec::new();
    let mut closing: Option<Instant> = None;
    let mut heard = Instant::now();
    let mut fed = Instant::now();
    loop {
        if control.load(Ordering::SeqCst) == ABORTING {
            let _ = socket.close(None);
            let _ = socket.flush();
            let _ = events.send(ScribeEvent::Closed);
            return;
        }
        if closing.is_none() {
            let samples = source.take();
            if !samples.is_empty() {
                fed = Instant::now();
            }
            pending.extend(downsampler.push(&samples));
            let sent = streamed.load(Ordering::Relaxed);
            // Never send past the session's minute limit.
            let room = max_samples.saturating_sub(sent) as usize;
            pending.truncate(room);
            let stop = if sent >= max_samples {
                Some(StopReason::Limit)
            } else if heard.elapsed() > SILENCE_LIMIT {
                Some(StopReason::Silence)
            } else if fed.elapsed() > NO_AUDIO_LIMIT {
                Some(StopReason::NoAudio)
            } else {
                None
            };
            let finishing = stop.is_some() || control.load(Ordering::SeqCst) == FINISHING;
            while pending.len() >= FRAME_SAMPLES || (finishing && !pending.is_empty()) {
                let count = pending.len().min(FRAME_SAMPLES);
                let frame: Vec<u8> = pending.drain(..count).flat_map(i16::to_le_bytes).collect();
                if socket.send(Message::binary(frame)).is_err() {
                    fail(&mut socket, "se cortó la conexión con Scribe".to_owned());
                    return;
                }
                streamed.fetch_add(count as u64, Ordering::Relaxed);
            }
            if finishing {
                if let Some(reason) = stop {
                    let _ = events.send(ScribeEvent::Stopping(reason));
                }
                let close = serde_json::json!({ "type": "session.close" }).to_string();
                if socket.send(Message::text(close)).is_err() {
                    fail(&mut socket, "se cortó la conexión con Scribe".to_owned());
                    return;
                }
                closing = Some(Instant::now());
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => match parse_event(text.as_str()) {
                Some(ScribeEvent::Closed) => break,
                Some(ScribeEvent::Failed(message)) => {
                    fail(&mut socket, message);
                    return;
                }
                Some(event) => {
                    if matches!(event, ScribeEvent::Speech(true) | ScribeEvent::Turn { .. }) {
                        heard = Instant::now();
                    }
                    let _ = events.send(event);
                }
                None => {}
            },
            Ok(Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => break,
            Err(error) => {
                fail(
                    &mut socket,
                    format!("la conexión con Scribe falló: {error}"),
                );
                return;
            }
        }
        if closing.is_some_and(|since| since.elapsed() > CLOSE_WAIT) {
            break;
        }
    }
    let _ = socket.close(None);
    let _ = socket.flush();
    let _ = events.send(ScribeEvent::Closed);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sessions share the one-session guard: their tests take turns.
    static SESSION_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn one_session_at_a_time() -> std::sync::MutexGuard<'static, ()> {
        SESSION_TESTS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    struct Samples {
        rate: u32,
        chunks: Vec<Vec<f32>>,
    }

    impl AudioSource for Samples {
        fn sample_rate(&self) -> u32 {
            self.rate
        }

        fn take(&mut self) -> Vec<f32> {
            if self.chunks.is_empty() {
                Vec::new()
            } else {
                self.chunks.remove(0)
            }
        }
    }

    #[test]
    fn tokens_come_from_the_panel_then_the_environment_and_never_print() {
        let environment =
            |name: &str| (name == TOKEN_VARIABLE).then(|| "Bearer env-token".to_owned());
        let (token, source) = ScribeToken::resolve("  ", || None, environment).unwrap();
        assert_eq!(
            (token.0.as_str(), source),
            ("env-token", TokenSource::Environment)
        );
        let stored = || ScribeToken::new("stored");
        let (token, source) = ScribeToken::resolve("", stored, environment).unwrap();
        assert_eq!((token.0.as_str(), source), ("stored", TokenSource::Vault));
        let (token, source) = ScribeToken::resolve(" typed ", stored, environment).unwrap();
        assert_eq!((token.0.as_str(), source), ("typed", TokenSource::Typed));
        assert!(ScribeToken::resolve("", || None, |_| None).is_none());
        assert_eq!(format!("{token:?}"), "ScribeToken(hidden)");
    }

    #[test]
    fn the_session_update_sends_the_language_and_opted_in_vocabulary() {
        let mut config = ScribeConfig {
            language: "es-ES".into(),
            phrases: Vec::new(),
            max_minutes: 60,
        };
        let event: serde_json::Value = serde_json::from_str(&session_update(&config)).unwrap();
        assert_eq!(event["type"], "session.update");
        assert_eq!(event["language"], "es-ES");
        assert_eq!(event["audio"]["format"], "pcm16");
        assert!(event.get("transcription").is_none());
        config.phrases = vec!["Resultados".into()];
        let event: serde_json::Value = serde_json::from_str(&session_update(&config)).unwrap();
        assert_eq!(event["transcription"]["phrase_list"][0], "Resultados");
    }

    #[test]
    fn server_events_become_turns_warnings_and_failures() {
        assert_eq!(
            parse_event(r#"{"type":"session.created","session_id":"x"}"#),
            Some(ScribeEvent::Ready)
        );
        assert_eq!(
            parse_event(
                r#"{"type":"transcription.completed","item_id":"1","transcript":" Hola a todos. ","audio_start_ms":1500,"audio_end_ms":2750,"transcription_latency_ms":300}"#
            ),
            Some(ScribeEvent::Turn {
                start: 1.5,
                end: 2.75,
                text: "Hola a todos.".into()
            })
        );
        assert_eq!(
            parse_event(
                r#"{"type":"transcription.completed","transcript":"  ","audio_start_ms":1}"#
            ),
            None
        );
        assert_eq!(
            parse_event(r#"{"type":"error","error":{"code":"x","message":"lento","fatal":false}}"#),
            Some(ScribeEvent::Warning("lento".into()))
        );
        assert_eq!(
            parse_event(r#"{"type":"error","error":{"message":"no"}}"#),
            Some(ScribeEvent::Failed("no".into()))
        );
        assert_eq!(
            parse_event(r#"{"type":"input_audio_buffer.speech_started","item_id":"1"}"#),
            Some(ScribeEvent::Speech(true))
        );
        assert_eq!(
            parse_event(r#"{"type":"session.closed","reason":"done"}"#),
            Some(ScribeEvent::Closed)
        );
        assert_eq!(parse_event("not json"), None);
        assert_eq!(parse_event(r#"{"type":"other"}"#), None);
    }

    #[test]
    fn language_tags_map_to_scribe_locales() {
        assert_eq!(locale_for("es-ES"), Some("es-ES"));
        assert_eq!(locale_for(" es "), Some("es-ES"));
        assert_eq!(locale_for("es_PE"), Some("es-ES"));
        assert_eq!(locale_for("EN-gb"), Some("en-US"));
        assert_eq!(locale_for("pt-PT"), Some("pt-PT"));
        assert_eq!(locale_for("pt"), Some("pt-BR"));
        assert_eq!(locale_for("ko-KR"), None);
        assert_eq!(locale_for(""), None);
    }

    #[test]
    fn statuses_explain_themselves_without_quoting_the_request() {
        assert!(describe_status(401).contains("token"));
        assert!(describe_status(403).contains("403"));
        assert_eq!(describe_status(500), "Zoom respondió 500 al conectar");
    }

    #[test]
    fn audio_downsamples_to_16_khz_pcm16_across_chunks() {
        let mut downsampler = Downsampler::new(48_000);
        let tone: Vec<f32> = (0..4_800)
            .map(|index| (index as f32 / 48.0 * std::f32::consts::TAU).sin() * 0.5)
            .collect();
        let mut output = downsampler.push(&tone[..1_001]);
        output.extend(downsampler.push(&tone[1_001..]));
        assert_eq!(output.len(), 1_600);
        let peak = output
            .iter()
            .map(|sample| sample.unsigned_abs())
            .max()
            .unwrap();
        assert!((15_000..=16_500).contains(&peak), "{peak}");
        // 16 kHz passes through, clamped to PCM16.
        let mut same = Downsampler::new(16_000);
        assert_eq!(same.push(&[0.0, 1.0, -2.0]), [0, i16::MAX, -i16::MAX]);
    }

    #[test]
    fn turns_join_the_part_being_said_when_they_started() {
        let positions = vec![
            (0.0, Some(("Portada".to_owned(), 0))),
            (4.0, Some(("Datos".to_owned(), 0))),
            (9.0, Some(("Datos".to_owned(), 1))),
            (12.0, None),
        ];
        let turns = vec![
            (0.5, "Hola.".to_owned()),
            (3.97, "Vamos a los datos.".to_owned()),
            (5.0, "Miren el gráfico.".to_owned()),
            (9.5, "Crece.".to_owned()),
            (6.0, "Sube.".to_owned()),
            (13.0, "Fuera.".to_owned()),
        ];
        assert_eq!(
            dictated_parts(&positions, &turns),
            [
                ("Portada".to_owned(), 0, "Hola.".to_owned()),
                (
                    "Datos".to_owned(),
                    0,
                    "Vamos a los datos. Miren el gráfico. Sube.".to_owned()
                ),
                ("Datos".to_owned(), 1, "Crece.".to_owned()),
            ]
        );
    }

    /// A local stand-in for Scribe Live: it checks the handshake and the
    /// configuration, transcribes once audio arrives and closes when asked.
    fn fake_scribe(
        reply: &'static str,
    ) -> (String, std::thread::JoinHandle<(bool, bool, usize, bool)>) {
        use tungstenite::Message;
        use tungstenite::handshake::server::{Request, Response};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut authorized = false;
            let mut socket =
                tungstenite::accept_hdr(stream, |request: &Request, mut response: Response| {
                    authorized = request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok())
                        == Some("Bearer secret-token");
                    let asked = request
                        .headers()
                        .get("sec-websocket-protocol")
                        .and_then(|value| value.to_str().ok())
                        == Some("live-asr");
                    if asked {
                        response
                            .headers_mut()
                            .insert("sec-websocket-protocol", "live-asr".parse().unwrap());
                    }
                    Ok(response)
                })
                .unwrap();
            let mut configured = false;
            let mut audio = 0;
            let mut replied = false;
            let mut closed = false;
            while let Ok(message) = socket.read() {
                match message {
                    Message::Text(text) if text.as_str().contains("session.update") => {
                        configured = text.as_str().contains("\"es-ES\"");
                        socket
                            .send(Message::text(
                                r#"{"type":"session.created","session_id":"s"}"#,
                            ))
                            .unwrap();
                    }
                    Message::Text(text) if text.as_str().contains("session.close") => {
                        closed = true;
                        socket
                            .send(Message::text(
                                r#"{"type":"session.closed","reason":"client"}"#,
                            ))
                            .unwrap();
                        break;
                    }
                    Message::Binary(bytes) => {
                        audio += bytes.len();
                        if !replied && audio >= FRAME_SAMPLES * 2 {
                            replied = true;
                            socket.send(Message::text(reply)).unwrap();
                        }
                    }
                    _ => {}
                }
            }
            (authorized, configured, audio, closed)
        });
        (url, server)
    }

    fn events_until_closed(session: &ScribeSession) -> Vec<ScribeEvent> {
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            events.extend(session.poll());
            if events.contains(&ScribeEvent::Closed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        events
    }

    #[test]
    fn a_session_streams_audio_reports_turns_and_closes_when_finished() {
        let _turn = one_session_at_a_time();
        let (url, server) = fake_scribe(
            r#"{"type":"transcription.completed","item_id":"1","transcript":"Hola","audio_start_ms":120,"audio_end_ms":480}"#,
        );
        // Half a second at 48 kHz, in uneven chunks.
        let samples = Samples {
            rate: 48_000,
            chunks: vec![vec![0.1; 10_000], vec![0.1; 14_000]],
        };
        let config = ScribeConfig {
            language: "es-ES".into(),
            phrases: Vec::new(),
            max_minutes: 60,
        };
        let session = ScribeSession::start_at(
            &url,
            ScribeToken::new("secret-token").unwrap(),
            config,
            samples,
        )
        .unwrap();
        // One session at a time.
        assert!(
            ScribeSession::start_at(
                &url,
                ScribeToken::new("x").unwrap(),
                ScribeConfig {
                    language: "es-ES".into(),
                    phrases: Vec::new(),
                    max_minutes: 1
                },
                Samples {
                    rate: 16_000,
                    chunks: Vec::new()
                }
            )
            .is_err()
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut events = Vec::new();
        while !events
            .iter()
            .any(|event| matches!(event, ScribeEvent::Turn { .. }))
            && std::time::Instant::now() < deadline
        {
            events.extend(session.poll());
            std::thread::sleep(Duration::from_millis(10));
        }
        session.finish();
        events.extend(events_until_closed(&session));
        assert!(events.contains(&ScribeEvent::Ready), "{events:?}");
        assert!(events.contains(&ScribeEvent::Turn {
            start: 0.12,
            end: 0.48,
            text: "Hola".into()
        }));
        assert_eq!(events.last(), Some(&ScribeEvent::Closed));
        assert!((session.streamed_seconds() - 0.5).abs() < 1e-9);
        drop(session);
        let (authorized, configured, audio, closed) = server.join().unwrap();
        assert!(authorized && configured && closed);
        assert_eq!(audio, 8_000 * 2, "every sample, once");
        // The session is over: another may start.
        assert!(!ACTIVE.load(Ordering::SeqCst));
    }

    #[test]
    fn a_session_stops_by_itself_at_its_minute_limit() {
        let _turn = one_session_at_a_time();
        let (url, server) =
            fake_scribe(r#"{"type":"input_audio_buffer.speech_started","item_id":"1"}"#);
        // Two minutes at 16 kHz, delivered at once, against a one-minute limit.
        let samples = Samples {
            rate: 16_000,
            chunks: vec![vec![0.0; 16_000 * 120]],
        };
        let config = ScribeConfig {
            language: "es-ES".into(),
            phrases: Vec::new(),
            max_minutes: 1,
        };
        let session = ScribeSession::start_at(
            &url,
            ScribeToken::new("secret-token").unwrap(),
            config,
            samples,
        )
        .unwrap();
        let events = events_until_closed(&session);
        assert!(
            events.contains(&ScribeEvent::Stopping(StopReason::Limit)),
            "{events:?}"
        );
        assert!((session.streamed_seconds() - 60.0).abs() < 1e-9);
        drop(session);
        let (_, _, audio, closed) = server.join().unwrap();
        assert!(closed);
        assert_eq!(audio, 16_000 * 60 * 2, "nothing past the limit");
    }

    #[test]
    fn a_refused_token_fails_without_retrying_or_quoting_it() {
        let _turn = one_session_at_a_time();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut attempts = 0;
            listener.set_nonblocking(true).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                if let Ok((mut stream, _)) = listener.accept() {
                    use std::io::{Read, Write};
                    attempts += 1;
                    stream.set_nonblocking(false).unwrap();
                    let mut request = [0_u8; 2048];
                    let _ = stream.read(&mut request);
                    let _ = stream.write_all(
                        b"HTTP/1.1 401 Unauthorized\r\ncontent-length: 0\r\nconnection: close\r\n\r\n",
                    );
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            attempts
        });
        let session = ScribeSession::start_at(
            &url,
            ScribeToken::new("secret-token").unwrap(),
            ScribeConfig {
                language: "es-ES".into(),
                phrases: Vec::new(),
                max_minutes: 60,
            },
            Samples {
                rate: 16_000,
                chunks: Vec::new(),
            },
        )
        .unwrap();
        let events = events_until_closed(&session);
        let failure = events
            .iter()
            .find_map(|event| match event {
                ScribeEvent::Failed(message) => Some(message.clone()),
                _ => None,
            })
            .expect("a failure");
        assert!(
            failure.contains("401") && !failure.contains("secret-token"),
            "{failure}"
        );
        drop(session);
        assert_eq!(server.join().unwrap(), 1, "no retries");
    }
}
