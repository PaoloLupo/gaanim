// Loads a .gaanim file from the drop zone, the file picker or `?src=URL`,
// and hands its bytes to the Rust player (the editor's own UI, in egui).
//
// A file from `?src=` downloads in pieces when its host serves byte ranges:
// the end of the file (the archive's directory and the bundle's tables)
// first, so playback starts at once; then the player asks for the chunks it
// needs (`gaanimFetch`) while the rest downloads in the background. A host
// without ranges gets the whole file in one download.
//
// The fragment points at a moment, as on video sites: `#segmento=5`,
// `#segmento=nombre&pausa=2`, `#pausa=12` or `#t=83.5`; `present` offers to
// start presenting.
//
// Presenting opens Presenter View as a second page (`?presenter=<session>`):
// the same player showing Presenter View alone. It takes the file from this
// page, and the two keep the same playhead over a BroadcastChannel.
//
// Full screen is the page's: a browser grants it only inside a click or key
// on this page, and opening Presenter View already spends the click that
// started the presentation.
//
// The audience page plays the file's audio with WebAudio, following the
// playhead the player reports every frame.
//
// A presentation with audience polls talks to its relay from the audience
// page: the player asks for requests and the results socket, and the page
// answers. `#clave=<key>` hands it the presenter key of a session that
// another computer claimed (`gaanim relay key`).
import init, {
  run,
  openBundle,
  openRemote,
  addBytes,
  setLink,
  setVolume,
  present,
  frameCount,
  linkReceive,
  relayResponse,
  relaySocketMessage,
  relaySocketState,
} from "./pkg/gaanim_web.js";

// Frames the player has drawn, for measuring its frame rate from the page.
window.gaanimFrameCount = () => frameCount();

const welcome = document.getElementById("welcome");
const drop = document.getElementById("drop");
const fileInput = document.getElementById("file");
const statusLine = document.getElementById("status");
const progress = document.getElementById("progress");
const canvas = document.getElementById("gaanim-canvas");
const toast = document.getElementById("toast");
const fillBar = document.getElementById("fill");
const buffering = document.getElementById("buffering");
const presentCard = document.getElementById("present-card");

const presenterSession = new URLSearchParams(location.search).get("presenter");
// Embedded in the playground (`?embed`): the parent page sends the bundles it
// records and hears what the player reports; there is nothing to drop here.
const embedded = new URLSearchParams(location.search).has("embed");
function tellParent(message) {
  if (embedded && window.parent !== window) window.parent.postMessage(message, location.origin);
}
const linkParams = new URLSearchParams(location.hash.slice(1));
// A presenter key from the link, kept out of the address bar from now on.
const linkKey = linkParams.get("clave");
if (linkKey !== null) {
  linkParams.delete("clave");
  const rest = linkParams.toString();
  history.replaceState(null, "", `${location.pathname}${location.search}${rest ? `#${rest}` : ""}`);
}
let ready = false;
// Work that needs the player, run once it has started.
let waiting = [];
function whenReady(task) {
  if (ready) task();
  else waiting.push(task);
}

// The channel to the other page of a presentation, once there is one.
let channel = null;
function connect(session) {
  channel = new BroadcastChannel(`gaanim-presenter-${session}`);
  channel.onmessage = (event) => {
    if (ready) linkReceive(event.data);
  };
}
window.gaanimLinkSend = (message) => channel?.postMessage(message);

let toastTimer = 0;
// Show `message`, with `link` in a field ready to copy, or a button that
// runs `action.run`, when given.
function notify(message, link = null, action = null) {
  toast.replaceChildren(message);
  if (link) {
    const field = document.createElement("input");
    field.readOnly = true;
    field.value = link;
    toast.append(field);
    field.select();
  }
  if (action) {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = action.label;
    button.addEventListener("click", () => {
      toast.hidden = true;
      action.run();
    });
    toast.append(button);
  }
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.hidden = true), link || action ? 20000 : 8000);
}
window.gaanimNotify = (message) => notify(message);

// ---------------------------------------------------------------------------
// Full screen
// ---------------------------------------------------------------------------

function fullscreenElement() {
  return document.fullscreenElement ?? document.webkitFullscreenElement ?? null;
}

// The whole page goes full screen, so its messages stay visible; the canvas
// follows its size.
function enterFullscreen() {
  const root = document.documentElement;
  const request = root.requestFullscreen ?? root.webkitRequestFullscreen;
  if (!request) {
    notify("Este navegador no permite la pantalla completa desde la página.");
    return;
  }
  Promise.resolve(request.call(root)).catch(() =>
    notify("El navegador no permitió la pantalla completa: pulsa otra vez el botón o F11."),
  );
}

function toggleFullscreen() {
  if (fullscreenElement()) (document.exitFullscreen ?? document.webkitExitFullscreen).call(document);
  else enterFullscreen();
}
// Called by the bar's and the dock's full screen buttons.
window.gaanimToggleFullscreen = toggleFullscreen;

// F11 is handled here, inside the key event, before the player sees it.
window.addEventListener(
  "keydown",
  (event) => {
    if (event.key !== "F11") return;
    event.preventDefault();
    event.stopImmediatePropagation();
    toggleFullscreen();
  },
  true,
);

// Called by the player when presenting starts or on P while presenting. It
// runs a frame after the click or key, still within the browser's allowance
// for opening windows.
let session = null;
window.gaanimOpenPresenter = () => {
  if (!window.gaanimBundle) return;
  if (!session) {
    session = crypto.randomUUID ? crypto.randomUUID() : String(Math.random()).slice(2);
    connect(session);
  }
  const url = new URL(location.pathname, location.href);
  url.searchParams.set("presenter", session);
  const popup = window.open(url, `gaanim-presenter-${session}`, "popup,width=1180,height=760");
  if (popup) {
    popup.focus();
    // Opening the window spends the click that could also have made this
    // page full screen; a browser grants one of the two per gesture.
    if (!fullscreenElement()) {
      notify(
        "Presenter View se abrió en otra ventana. Lleva esta a la pantalla del público y ponla en pantalla completa (también con F11 o el botón del panel inferior).",
        null,
        { label: "Pantalla completa", run: enterFullscreen },
      );
    }
  } else {
    notify("El navegador bloqueó la ventana de Presenter View: permite las ventanas emergentes de esta página y pulsa P.");
  }
};

// Called by the bar's "Copiar enlace": a link to this page and file, at the
// moment `fragment` names (empty for the start).
window.gaanimCopyLink = (fragment) => {
  const url = window.gaanimBundle?.url;
  if (!url) {
    notify("Este archivo se abrió desde tu equipo, así que no tiene enlace. Publícalo en la web y ábrelo con ?src= (consulta «Cómo compartir un .gaanim»).");
    return;
  }
  // Keep the file's address readable: escape only what would end it.
  const src = url.replace(/[%&#+ ]/g, encodeURIComponent);
  const link = `${location.origin}${location.pathname}?src=${src}${fragment ? `#${fragment}` : ""}`;
  const copied = navigator.clipboard?.writeText(link);
  if (!copied) {
    notify("Copia el enlace:", link);
    return;
  }
  copied.then(
    () => notify(fragment ? "Enlace a este instante copiado." : "Enlace copiado."),
    () => notify("Copia el enlace:", link),
  );
};

function say(message, error = false) {
  statusLine.textContent = message;
  statusLine.classList.toggle("error", error);
}

// Called by the player: "ready", "opened", "damaged" (not a .gaanim, or
// damaged) or "error".
window.gaanimStatus = (kind, message) => {
  tellParent({ type: "gaanim:status", kind, message: message ?? null });
  if (kind === "ready") {
    ready = true;
    const tasks = waiting;
    waiting = [];
    for (const task of tasks) task();
  } else if (kind === "opened") {
    welcome.hidden = true;
    canvas.focus();
    showSoundHint();
    if (remote) {
      remote.opened = true;
      downloads.delete(remote.name);
      showDownloads();
      showBuffering(remote);
      fill(remote);
    }
    if (presenterSession === null && linkParams.has("present")) presentCard.hidden = false;
  } else if (kind === "damaged" && remote) {
    // A host that compresses the file on the fly breaks byte ranges: read
    // the file whole once before calling it damaged.
    const { url, name } = remote;
    stopRemote();
    playWhole(url, name);
  } else {
    stopRemote();
    welcome.hidden = false;
    say(message, true);
  }
};

function play(name, bytes, url = null) {
  // Presenter View's page takes the file from here.
  window.gaanimBundle = { name, bytes, url };
  if (!downloads.size) say(`Abriendo ${name}…`);
  whenReady(() => openBundle(name, bytes));
}

async function playFile(file) {
  if (!file.name.toLowerCase().endsWith(".gaanim")) {
    say(`«${file.name}» no es un archivo .gaanim.`, true);
    return;
  }
  stopRemote();
  play(file.name, new Uint8Array(await file.arrayBuffer()));
}

// Downloads in progress, shown together on the progress bar.
const downloads = new Map();

function showDownloads() {
  let received = 0;
  let total = 0;
  const parts = [];
  for (const [label, entry] of downloads) {
    received += entry.received;
    total += entry.total;
    parts.push(`${label} ${(entry.received / 1e6).toFixed(1)}${entry.total ? ` / ${(entry.total / 1e6).toFixed(1)}` : ""} MB`);
  }
  progress.hidden = downloads.size === 0;
  if (total) progress.value = received / total;
  else progress.removeAttribute("value");
  if (parts.length) say(`Descargando ${parts.join(" · ")}`);
}

// An HTTP error with its status, for `downloadError`.
function httpError(response) {
  return Object.assign(new Error(`HTTP ${response.status}`), { status: response.status });
}

async function download(url, label) {
  const response = await fetch(url);
  if (!response.ok) throw httpError(response);
  const entry = { received: 0, total: Number(response.headers.get("Content-Length")) || 0 };
  downloads.set(label, entry);
  const reader = response.body.getReader();
  const chunks = [];
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      entry.received += value.length;
      showDownloads();
    }
  } finally {
    downloads.delete(label);
    showDownloads();
  }
  const bytes = new Uint8Array(entry.received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

// What went wrong downloading the file, for the viewer.
function downloadError(error) {
  if (error.status === 404 || error.status === 410) {
    return "No se encontró el archivo: revisa la dirección del enlace.";
  }
  if (error.status) return `El sitio donde está el archivo respondió con un error (HTTP ${error.status}).`;
  if (!navigator.onLine) return "No hay conexión a internet.";
  return "El sitio donde está el archivo no permite abrirlo desde otra página (CORS), o no hay conexión. Funcionan GitHub Pages, raw.githubusercontent.com o un sitio propio con CORS: consulta «Cómo compartir un .gaanim».";
}

// The module ships gzip-compressed so it downloads small from any host; a
// server that already decoded it hands over the plain module.
async function gunzip(bytes) {
  if (bytes[0] !== 0x1f || bytes[1] !== 0x8b) return bytes;
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function startPlayer() {
  let module;
  try {
    module = await gunzip(await download("pkg/gaanim_web_bg.wasm.gz", "reproductor"));
  } catch (error) {
    say(`No se pudo descargar el reproductor: ${error.message}. Recarga la página.`, true);
    return;
  }
  if (!downloads.size && window.gaanimBundle) say(`Abriendo ${window.gaanimBundle.name}…`);
  try {
    await init({ module_or_path: module });
    // The page keeps running after this: Bevy hands control to the browser
    // by throwing once its loop is scheduled.
    try {
      run(presenterSession !== null);
    } catch (error) {
      if (!String(error).includes("Using exceptions for control flow")) throw error;
    }
  } catch (error) {
    say(`No se pudo iniciar el reproductor: ${error}`, true);
  }
}

// ---------------------------------------------------------------------------
// Downloading in pieces
// ---------------------------------------------------------------------------

// Files at least this large download in pieces when the host allows it.
const PIECES_FROM = 2 << 20;
// The end of the file fetched first: the archive's directory and, usually,
// every table.
const TAIL = 1 << 20;
// Size of each background request.
const FILL_BLOCK = 4 << 20;
// Requests closer than this are joined into one.
const JOIN_GAP = 64 << 10;

// The file downloading in pieces: { url, name, length, generation, have,
// inflight, retryAt, urgent, opened }. `have` and `inflight` hold
// [start, end) byte ranges.
let remote = null;

function stopRemote() {
  if (remote) downloads.delete(remote.name);
  remote = null;
  fillBar.hidden = true;
  buffering.hidden = true;
}

function addRange(ranges, start, end) {
  ranges.push([start, end]);
  ranges.sort((a, b) => a[0] - b[0]);
  const merged = [];
  for (const range of ranges) {
    const last = merged[merged.length - 1];
    if (last && range[0] <= last[1]) last[1] = Math.max(last[1], range[1]);
    else merged.push([...range]);
  }
  ranges.splice(0, ranges.length, ...merged);
}

// The parts of [start, end) that none of `lists` covers.
function gaps(start, end, ...lists) {
  const covered = lists.flat().sort((a, b) => a[0] - b[0]);
  const result = [];
  let at = start;
  for (const [from, to] of covered) {
    if (to <= at) continue;
    if (from >= end) break;
    if (from > at) result.push([at, Math.min(from, end)]);
    at = Math.max(at, to);
    if (at >= end) break;
  }
  if (at < end) result.push([at, end]);
  return result;
}

// Bytes [start, end) of `url`; `onBytes(count)` follows the download.
async function fetchRange(url, start, end, onBytes = null) {
  const response = await fetch(url, { headers: { Range: `bytes=${start}-${end - 1}` } });
  if (response.status !== 206) {
    throw response.ok ? new RangeError("byte ranges not supported") : httpError(response);
  }
  const bytes = new Uint8Array(end - start);
  let received = 0;
  const reader = response.body.getReader();
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (received + value.length > bytes.length) throw new RangeError("byte ranges not supported");
    bytes.set(value, received);
    received += value.length;
    onBytes?.(value.length);
  }
  if (received !== bytes.length) throw new RangeError("byte ranges not supported");
  return bytes;
}

function showFill(r) {
  const have = r.have.reduce((sum, [start, end]) => sum + end - start, 0);
  fillBar.hidden = have >= r.length;
  fillBar.style.width = `${(100 * have) / r.length}%`;
}

let bufferingTimer = 0;
// Shown once the file is open, while the frame on screen waits for its
// bytes; before that, the welcome card shows the download.
function showBuffering(r) {
  clearTimeout(bufferingTimer);
  if (r.opened && r.urgent > 0) bufferingTimer = setTimeout(() => (buffering.hidden = remote !== r), 300);
  else buffering.hidden = true;
}

async function request(r, start, end, urgent) {
  const entry = [start, end];
  r.inflight.push(entry);
  if (urgent) {
    r.urgent += 1;
    showBuffering(r);
  }
  // Until the file opens, the welcome card shows what opening downloads.
  const opening = r.opened ? null : downloads.get(r.name);
  if (opening) {
    opening.total += end - start;
    showDownloads();
  }
  const onBytes = opening
    ? (count) => {
        opening.received += count;
        showDownloads();
      }
    : null;
  try {
    const bytes = await fetchRange(r.url, start, end, onBytes);
    if (remote !== r) return;
    addBytes(r.generation, start, bytes);
    addRange(r.have, start, end);
    showFill(r);
  } catch (error) {
    if (remote === r && Date.now() >= r.retryAt) {
      notify("Se interrumpió la descarga del archivo; reintentando…");
    }
    r.retryAt = Date.now() + 2000;
  } finally {
    r.inflight.splice(r.inflight.indexOf(entry), 1);
    if (urgent) {
      r.urgent -= 1;
      showBuffering(r);
    }
  }
}

// Called by the player with the byte ranges it needs, as start and end
// pairs; `urgent` when the frame on screen waits for them.
window.gaanimFetch = (flat, urgent) => {
  const r = remote;
  if (!r || Date.now() < r.retryAt) return;
  const wanted = [];
  for (let index = 0; index + 1 < flat.length; index += 2) wanted.push([flat[index], flat[index + 1]]);
  wanted.sort((a, b) => a[0] - b[0]);
  const joined = [];
  for (const range of wanted) {
    const last = joined[joined.length - 1];
    if (last && range[0] <= last[1] + JOIN_GAP) last[1] = Math.max(last[1], range[1]);
    else joined.push([...range]);
  }
  for (const [start, end] of joined) {
    for (const [from, to] of gaps(start, end, r.have, r.inflight)) request(r, from, to, urgent);
  }
};

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Download the rest of the file, in order, one block at a time.
async function fill(r) {
  while (remote === r) {
    if (!gaps(0, r.length, r.have).length) break;
    const wait = r.retryAt - Date.now();
    if (wait > 0) {
      await sleep(wait);
      continue;
    }
    const gap = gaps(0, r.length, r.have, r.inflight)[0];
    if (!gap) {
      await sleep(200);
      continue;
    }
    await request(r, gap[0], Math.min(gap[1], gap[0] + FILL_BLOCK), false);
  }
}

function fileName(url) {
  return decodeURIComponent(new URL(url, location.href).pathname.split("/").pop() || "archivo.gaanim");
}

async function playWhole(url, name) {
  let bytes;
  try {
    bytes = await download(url, name);
  } catch (error) {
    say(downloadError(error), true);
    return;
  }
  play(name, bytes, url);
}

async function playUrl(url) {
  url = new URL(url, location.href).href;
  const name = fileName(url);
  stopRemote();
  say(`Abriendo ${name}…`);
  let length = 0;
  try {
    const head = await fetch(url, { method: "HEAD" });
    if (head.status === 404 || head.status === 410) throw httpError(head);
    if (head.ok) length = Number(head.headers.get("Content-Length")) || 0;
  } catch (error) {
    say(downloadError(error), true);
    return;
  }
  if (length >= PIECES_FROM) {
    const start = Math.max(0, length - TAIL);
    let tail = null;
    try {
      tail = await fetchRange(url, start, length);
    } catch {
      // No byte ranges (or not across origins): download it whole.
    }
    // A .gaanim ends with the ZIP end record. A host that compresses files
    // on the fly reports the compressed length, so this tail is not the end.
    const end = tail && tail.length - 22;
    if (tail && !(end >= 0 && tail[end] === 0x50 && tail[end + 1] === 0x4b && tail[end + 2] === 5 && tail[end + 3] === 6)) {
      tail = null;
    }
    if (tail) {
      const r = {
        url,
        name,
        length,
        generation: null,
        have: [[start, length]],
        inflight: [],
        retryAt: 0,
        urgent: 0,
        opened: false,
      };
      remote = r;
      downloads.set(name, { received: length - start, total: length - start });
      showDownloads();
      window.gaanimBundle = { name, url };
      showFill(r);
      whenReady(() => {
        if (remote === r) r.generation = openRemote(name, length, start, tail);
      });
      return;
    }
  }
  await playWhole(url, name);
}

// ---------------------------------------------------------------------------
// Audio
// ---------------------------------------------------------------------------

// The open file's audio: its tracks (as the player hands them over), the
// decoded files, and the sources playing. `anchor` says which playhead time
// the context's clock stood for when they were scheduled.
const audio = {
  ctx: null,
  // Every source goes through this: the bar's volume and mute.
  master: null,
  volume: null,
  tracks: [],
  buffers: new Map(),
  sources: new Map(),
  anchor: null,
  generation: 0,
};
const soundHint = document.getElementById("sound");
// Sources start this far ahead of the playhead, in timeline seconds.
const AUDIO_HORIZON = 10;
// A playhead further than this from the audio clock reschedules every
// source: a seek, or drift between the two clocks. Frame jitter must stay
// under it, or sounds would cut and restart.
const AUDIO_TOLERANCE = 0.12;

function audioContext() {
  if (!audio.ctx) {
    const Context = window.AudioContext ?? window.webkitAudioContext;
    if (!Context) return null;
    audio.ctx = new Context();
    audio.ctx.onstatechange = showSoundHint;
    audio.master = audio.ctx.createGain();
    if (audio.volume) audio.master.gain.value = audio.volume.muted ? 0 : audio.volume.level;
    audio.master.connect(audio.ctx.destination);
  }
  return audio.ctx;
}

// A browser starts audio only after a click or key on the page.
function showSoundHint() {
  soundHint.hidden = !(
    audio.tracks.length && audio.ctx && audio.ctx.state !== "running" && welcome.hidden
  );
}
for (const type of ["pointerdown", "keydown", "touchend"]) {
  window.addEventListener(
    type,
    () => {
      if (audio.ctx && audio.ctx.state === "suspended") audio.ctx.resume();
    },
    true,
  );
}

function stopAudio() {
  for (const { node, gain } of audio.sources.values()) {
    try {
      node.stop();
    } catch {
      // Not started yet, or already stopped.
    }
    node.disconnect();
    gain.disconnect();
  }
  audio.sources.clear();
  audio.anchor = null;
}

window.gaanimAudioTracks = (json) => {
  stopAudio();
  audio.generation += 1;
  audio.tracks = JSON.parse(json);
  audio.buffers.clear();
  if (audio.tracks.length) audioContext();
  showSoundHint();
};

window.gaanimAudioMedia = (entry, bytes) => {
  const ctx = audioContext();
  if (!ctx) return;
  const generation = audio.generation;
  const data = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength);
  ctx.decodeAudioData(data).then(
    (buffer) => {
      if (audio.generation !== generation) return;
      audio.buffers.set(entry, buffer);
    },
    () => notify("Este navegador no puede reproducir uno de los audios del archivo."),
  );
};

// Seconds of the track on the timeline: one pass of its clip, and how long
// it sounds (a looping track sounds on and on).
function trackSpan(track, buffer) {
  const sourceLength = track.sourceDuration ?? Math.max(0, buffer.duration - track.offset);
  const clip = sourceLength / track.speed;
  const active = track.duration != null ? Math.min(track.duration, clip) : clip;
  return { sourceLength, clip, active };
}

// The track's volume at timeline time `t`, with its fades, as the desktop
// preview computes it.
function trackLevel(track, active, t) {
  const elapsed = t - track.start;
  let level = 1;
  if (track.fadeIn > 0) level = Math.min(level, Math.max(0, Math.min(1, elapsed / track.fadeIn)));
  if (track.fadeOut > 0) level = Math.min(level, Math.max(0, Math.min(1, (active - elapsed) / track.fadeOut)));
  return track.volume * level;
}

// Start the sources of tracks that sound between `time` and the horizon;
// the context's clock `now` stands for `time`, running `rate` times slower.
function scheduleAudio(time, now, rate) {
  const ctx = audio.ctx;
  const at = (t) => now + (t - time) / rate;
  audio.tracks.forEach((track, index) => {
    if (audio.sources.has(index) || !(track.speed > 0)) return;
    const buffer = audio.buffers.get(track.media);
    if (!buffer) return;
    const { sourceLength, clip, active } = trackSpan(track, buffer);
    if (!(clip > 0)) return;
    const end = track.looping ? Infinity : track.start + active;
    if (end <= time + 1e-3 || track.start > time + AUDIO_HORIZON) return;

    const from = Math.max(time, track.start);
    const elapsed = from - track.start;
    const node = ctx.createBufferSource();
    node.buffer = buffer;
    node.playbackRate.value = track.speed * rate;
    if (track.looping) {
      node.loop = true;
      node.loopStart = track.offset;
      node.loopEnd = track.offset + sourceLength;
    }
    const gain = ctx.createGain();
    const level = gain.gain;
    level.setValueAtTime(trackLevel(track, active, from), at(from));
    const fadeInEnd = track.start + track.fadeIn;
    if (track.fadeIn > 0 && fadeInEnd > from) {
      level.linearRampToValueAtTime(trackLevel(track, active, fadeInEnd), at(fadeInEnd));
    }
    if (track.fadeOut > 0) {
      const fadeOutStart = Math.max(from, track.start + active - track.fadeOut, fadeInEnd);
      level.setValueAtTime(trackLevel(track, active, fadeOutStart), at(fadeOutStart));
      level.linearRampToValueAtTime(0, at(track.start + active));
    }
    node.connect(gain).connect(audio.master);
    const into = track.looping ? elapsed % clip : elapsed;
    node.start(at(from), track.offset + into * track.speed);
    if (Number.isFinite(end)) node.stop(at(end));
    node.onended = () => {
      if (audio.sources.get(index)?.node === node) {
        audio.sources.delete(index);
        gain.disconnect();
      }
    };
    audio.sources.set(index, { node, gain });
  });
}

const VOLUME_KEY = "gaanim-volume";

// The volume the bar shows, as this browser remembered it.
function rememberedVolume() {
  try {
    const saved = JSON.parse(storage()?.getItem(VOLUME_KEY) ?? "null");
    if (saved && Number.isFinite(saved.level)) return { level: saved.level, muted: !!saved.muted };
  } catch {
    // Nothing usable saved.
  }
  return null;
}

// Follow the bar's volume, and remember it.
function followVolume(level, muted) {
  const previous = audio.volume;
  if (previous && previous.level === level && previous.muted === muted) return;
  audio.volume = { level, muted };
  if (audio.master) {
    // A short ramp, so dragging the slider does not click.
    audio.master.gain.setTargetAtTime(muted ? 0 : level, audio.ctx.currentTime, 0.02);
  }
  if (previous) {
    try {
      storage()?.setItem(VOLUME_KEY, JSON.stringify(audio.volume));
    } catch {
      // Private browsing: the volume lasts this visit.
    }
  }
}

// Called by the player every frame with the playhead and the volume.
window.gaanimAudioState = (time, playing, rate, level, muted) => {
  followVolume(level, muted);
  const ctx = audio.ctx;
  if (!ctx || !audio.tracks.length) return;
  if (!playing || ctx.state !== "running" || document.hidden || !(rate > 0)) {
    if (audio.anchor) stopAudio();
    return;
  }
  const now = ctx.currentTime;
  const anchor = audio.anchor;
  const expected = anchor && anchor.time + (now - anchor.at) * anchor.rate;
  if (!anchor || anchor.rate !== rate || Math.abs(expected - time) > AUDIO_TOLERANCE) {
    stopAudio();
    audio.anchor = { time, at: now, rate };
  }
  // Schedule from the anchor, so sources keep the clock they started on.
  const { time: anchorTime, at: anchorAt } = audio.anchor;
  scheduleAudio(anchorTime + (now - anchorAt) * rate, now, rate);
};

// A hidden page stops drawing, and so stops moving the playhead.
document.addEventListener("visibilitychange", () => {
  if (document.hidden) stopAudio();
});

// ---------------------------------------------------------------------------
// Audience polls
// ---------------------------------------------------------------------------

function storage() {
  try {
    return window.localStorage;
  } catch {
    return null;
  }
}

// The presenter key of a session: the link's, or one this browser made for
// it. The relay accepts the first key that claims a session.
window.gaanimRelayKey = (relay, code) => {
  const name = `gaanim-relay-key:${relay}/s/${code}`;
  const store = storage();
  let key = linkKey ?? store?.getItem(name) ?? null;
  if (!key) {
    const bytes = crypto.getRandomValues(new Uint8Array(32));
    key = [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
  }
  try {
    store?.setItem(name, key);
  } catch {
    // Private browsing: the key lasts this visit.
  }
  return key;
};

window.gaanimRelayRequest = (id, method, url, key, type, body) => {
  const headers = { authorization: `Bearer ${key}` };
  const init = { method, headers, cache: "no-store", keepalive: id === 0 };
  if (body.length) {
    headers["content-type"] = type;
    init.body = body;
  }
  fetch(url, init).then(
    async (response) => relayResponse(id, response.status, await response.text().catch(() => "")),
    () => relayResponse(id, 0, ""),
  );
};

// The relay pushes results on this socket; it reconnects with backoff, and
// keeps itself alive with "ping".
let relaySocket = null;
window.gaanimRelaySocket = (url, key) => {
  window.gaanimRelayClose();
  relaySocket = { url, key, backoff: 1000, stopped: false, ws: null, timer: 0 };
  connectRelay(relaySocket);
};
window.gaanimRelayClose = () => {
  if (!relaySocket) return;
  relaySocket.stopped = true;
  clearTimeout(relaySocket.timer);
  relaySocket.ws?.close();
  relaySocket = null;
};

function connectRelay(socket) {
  if (socket.stopped) return;
  const ws = new WebSocket(socket.url, ["gaanim-presenter", `key.${socket.key}`]);
  socket.ws = ws;
  let ping = 0;
  ws.onopen = () => {
    socket.backoff = 1000;
    ping = setInterval(() => ws.readyState === WebSocket.OPEN && ws.send("ping"), 25000);
  };
  ws.onmessage = (event) => {
    if (typeof event.data === "string" && event.data !== "pong") relaySocketMessage(event.data);
  };
  ws.onclose = () => {
    clearInterval(ping);
    if (socket.stopped) return;
    relaySocketState(false);
    socket.timer = setTimeout(() => connectRelay(socket), socket.backoff);
    socket.backoff = Math.min(socket.backoff * 2, 30000);
  };
}

window.gaanimDownload = (name, bytes) => {
  const url = URL.createObjectURL(new Blob([bytes]));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
};

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

fileInput.addEventListener("change", () => {
  if (fileInput.files[0]) playFile(fileInput.files[0]);
});
for (const target of embedded ? [] : [drop, document.body]) {
  target.addEventListener("dragover", (event) => {
    event.preventDefault();
    drop.classList.add("over");
  });
  target.addEventListener("dragleave", () => drop.classList.remove("over"));
  target.addEventListener("drop", (event) => {
    event.preventDefault();
    drop.classList.remove("over");
    const file = event.dataTransfer.files[0];
    if (file) playFile(file);
  });
}

presentCard.querySelector("button").addEventListener("click", () => {
  presentCard.hidden = true;
  present();
});
presentCard.querySelector("a").addEventListener("click", (event) => {
  event.preventDefault();
  presentCard.hidden = true;
  canvas.focus();
});

if (!navigator.gpu) {
  drop.hidden = true;
  say("Este navegador no tiene WebGPU, que el reproductor necesita. Usa una versión reciente de Chrome, Edge, Safari o Firefox.", true);
} else if (presenterSession !== null) {
  document.title = "Presenter View — Gaanim";
  drop.hidden = true;
  connect(presenterSession);
  const source = window.opener?.gaanimBundle;
  if (source?.bytes) {
    startPlayer();
    play(source.name, new Uint8Array(source.bytes), source.url);
  } else if (source?.url) {
    startPlayer();
    playUrl(source.url);
  } else {
    say("Presenter View se abre desde el reproductor: abre allí tu archivo, presenta y pulsa P.", true);
  }
} else if (embedded) {
  document.body.classList.add("embedded");
  drop.hidden = true;
  say("Ejecuta el código para ver la escena aquí.");
  startPlayer();
  const volume = rememberedVolume();
  if (volume) whenReady(() => setVolume(volume.level, volume.muted));
  window.addEventListener("message", (event) => {
    if (event.origin !== location.origin || event.source !== window.parent) return;
    const message = event.data;
    if (message?.type === "gaanim:open" && message.bytes) {
      play(message.name ?? "escena.gaanim", new Uint8Array(message.bytes));
    }
  });
  tellParent({ type: "gaanim:status", kind: "loaded", message: null });
} else {
  startPlayer();
  const volume = rememberedVolume();
  if (volume) whenReady(() => setVolume(volume.level, volume.muted));
  if (location.hash) whenReady(() => setLink(location.hash));
  // A new fragment, typed or from another link, moves the playhead.
  window.addEventListener("hashchange", () => whenReady(() => setLink(location.hash)));
  const src = new URLSearchParams(location.search).get("src");
  if (src) playUrl(src);
}
