// Loads a .gaanim file from the drop zone, the file picker or `?src=URL`,
// and hands its bytes to the Rust player (the editor's own UI, in egui).
import init, { openBundle, frameCount } from "./pkg/gaanim_web.js";

// Frames the player has drawn, for measuring its frame rate from the page.
window.gaanimFrameCount = () => frameCount();

const welcome = document.getElementById("welcome");
const drop = document.getElementById("drop");
const fileInput = document.getElementById("file");
const statusLine = document.getElementById("status");
const progress = document.getElementById("progress");
const canvas = document.getElementById("gaanim-canvas");

let ready = false;
let pending = null;

function say(message, error = false) {
  statusLine.textContent = message;
  statusLine.classList.toggle("error", error);
}

// Called by the player: "ready", "opened" or "error".
window.gaanimStatus = (kind, message) => {
  if (kind === "ready") {
    ready = true;
    if (pending) {
      openBundle(pending.name, pending.bytes);
      pending = null;
    }
  } else if (kind === "opened") {
    welcome.hidden = true;
    canvas.focus();
  } else if (kind === "error") {
    welcome.hidden = false;
    say(`No se pudo abrir el archivo: ${message}`, true);
  }
};

function play(name, bytes) {
  if (!downloads.size) say(`Abriendo ${name}…`);
  if (ready) {
    openBundle(name, bytes);
  } else {
    pending = { name, bytes };
  }
}

async function playFile(file) {
  if (!file.name.toLowerCase().endsWith(".gaanim")) {
    say(`«${file.name}» no es un archivo .gaanim.`, true);
    return;
  }
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

async function download(url, label) {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
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
  say(pending ? `Abriendo ${pending.name}…` : "");
  try {
    await init({ module_or_path: module });
  } catch (error) {
    say(`No se pudo iniciar el reproductor: ${error}`, true);
  }
}

async function playUrl(url) {
  const name = decodeURIComponent(new URL(url, location.href).pathname.split("/").pop() || "archivo.gaanim");
  let bytes;
  try {
    bytes = await download(url, name);
  } catch (error) {
    say(
      error instanceof TypeError
        ? "No se pudo descargar el archivo: el sitio donde está no permite abrirlo desde otra página (CORS) o no hay conexión."
        : `No se pudo descargar el archivo (${error.message}).`,
      true,
    );
    return;
  }
  play(name, bytes);
}

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

if (!navigator.gpu) {
  drop.hidden = true;
  say("Este navegador no tiene WebGPU, que el reproductor necesita. Usa una versión reciente de Chrome, Edge, Safari o Firefox.", true);
} else {
  startPlayer();
  const src = new URLSearchParams(location.search).get("src");
  if (src) playUrl(src);
}
