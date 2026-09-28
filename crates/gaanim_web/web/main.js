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
  console.info(`gaanim: ${kind} ${message}`);
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
    progress.hidden = true;
    say(`No se pudo abrir el archivo: ${message}`, true);
  }
};

function play(name, bytes) {
  say(`Abriendo ${name}…`);
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

async function playUrl(url) {
  let response;
  try {
    response = await fetch(url);
  } catch {
    say("No se pudo descargar el archivo: el sitio donde está no permite abrirlo desde otra página (CORS) o no hay conexión.", true);
    return;
  }
  if (!response.ok) {
    say(`No se pudo descargar el archivo (${response.status}).`, true);
    return;
  }
  const total = Number(response.headers.get("Content-Length")) || 0;
  const reader = response.body.getReader();
  const chunks = [];
  let received = 0;
  progress.hidden = false;
  progress.removeAttribute("value");
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.length;
    if (total) progress.value = received / total;
    say(`Descargando… ${(received / 1e6).toFixed(1)} MB`);
  }
  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  progress.hidden = true;
  play(decodeURIComponent(new URL(url, location.href).pathname.split("/").pop() || "archivo.gaanim"), bytes);
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
  const src = new URLSearchParams(location.search).get("src");
  if (src) playUrl(src);
  init().catch((error) => say(`No se pudo iniciar el reproductor: ${error}`, true));
}
