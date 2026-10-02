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
import init, {
  run,
  openBundle,
  openRemote,
  addBytes,
  setLink,
  present,
  frameCount,
  linkReceive,
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
const linkParams = new URLSearchParams(location.hash.slice(1));
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
// Show `message`, with `link` in a field ready to copy when given.
function notify(message, link = null) {
  toast.replaceChildren(message);
  if (link) {
    const field = document.createElement("input");
    field.readOnly = true;
    field.value = link;
    toast.append(field);
    field.select();
  }
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.hidden = true), link ? 20000 : 8000);
}
window.gaanimNotify = (message) => notify(message);

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
    if (!document.fullscreenElement) {
      notify("Presenter View se abrió en otra ventana. Lleva esta a la pantalla del público y ponla en pantalla completa con F11 o con el botón de la barra.");
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
  if (kind === "ready") {
    ready = true;
    const tasks = waiting;
    waiting = [];
    for (const task of tasks) task();
  } else if (kind === "opened") {
    welcome.hidden = true;
    canvas.focus();
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
// Page
// ---------------------------------------------------------------------------

fileInput.addEventListener("change", () => {
  if (fileInput.files[0]) playFile(fileInput.files[0]);
});
for (const target of [drop, document.body]) {
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
} else {
  startPlayer();
  if (location.hash) whenReady(() => setLink(location.hash));
  // A new fragment, typed or from another link, moves the playhead.
  window.addEventListener("hashchange", () => whenReady(() => setLink(location.hash)));
  const src = new URLSearchParams(location.search).get("src");
  if (src) playUrl(src);
}
