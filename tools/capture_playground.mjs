// Screenshots of the playground for its documentation page
// (docs/content/empezar/playground.typ), taken from a real session: a
// headless Edge or Chrome opens the playground, imports a project from a ZIP,
// runs it, breaks it, records it, and opens the dialogs, as a person would.
//
// Usage (the playground served on a local port, e.g. by `just playground`):
//   node tools/capture_playground.mjs [--url http://localhost:8001/]
//        [--out docs/assets/playground] [--browser <msedge.exe or chrome>]
//
// Node 22 or newer (its WebSocket drives the browser through the DevTools
// protocol, so nothing is installed). The browser needs WebGPU; the profile is
// a new temporary one, so the playground starts as a first visit does. The
// video export and the HTML page are checked too: the MP4 and the page they
// download must exist.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const args = Object.fromEntries(
  process.argv.slice(2).reduce((pairs, value, index, all) => (value.startsWith("--") ? [...pairs, [value.slice(2), all[index + 1]]] : pairs), []),
);
const URL_ = args.url ?? "http://localhost:8001/";
const OUT = resolve(args.out ?? join(ROOT, "docs", "assets", "playground"));
const BROWSERS = [
  args.browser,
  "C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Microsoft\\Edge\\Application\\msedge.exe",
  "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/microsoft-edge",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
].filter(Boolean);
const BROWSER = BROWSERS.find((path) => existsSync(path));
if (!BROWSER) throw new Error("no Edge or Chrome found; pass --browser");
const WIDTH = 1280;
const HEIGHT = 800;
const SCALE = 1.5;

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));

// ---------------------------------------------------------------------------
// The project the screenshots show, as the ZIP a person would open
// ---------------------------------------------------------------------------

const MAIN = `from gaanim import BLUE, GOLD, Scene, Theme
from lib.formas import fila_de_cuadros

# La fuente está en assets/fonts: el tema la usa para el texto.
scene = Scene(frame=(16, 9), theme=Theme("technical", font_dir="assets/fonts", fonts={"text": "Victor Mono"}))
titulo = scene.text("Mi animación en el navegador").move_to(0, 2.6)
cuadros = fila_de_cuadros(scene, 5, colores=(BLUE, GOLD))

scene.play([titulo.animate.write().duration(1.0)])
scene.play([cuadro.animate.fade_in().duration(0.5) for cuadro in cuadros])
scene.play([cuadro.animate.shift_by(0, -1.2).duration(0.8) for cuadro in cuadros[::2]])
scene.render()
`;
const FORMAS = `"""Figuras que main.py reutiliza."""


def fila_de_cuadros(scene, cantidad, lado=1.2, colores=()):
    """Una fila de cuadrados centrada, con los colores alternados."""
    paso = lado * 1.6
    inicio = -paso * (cantidad - 1) / 2
    return [
        scene.geometry.rect(lado, lado).fill(colores[i % len(colores)]).move_to(inicio + i * paso, 0)
        for i in range(cantidad)
    ]
`;
const BROKEN = FORMAS.replace("scene.geometry.rect(lado, lado)", "scene.geometry.rectangulo(lado, lado)");
const NOTES = "# Mi animación\n\nUna fila de cuadros y un título con una fuente propia.\n";
const PYPROJECT = '[project]\nname = "mi-animacion"\nversion = "0.1.0"\nrequires-python = ">=3.14"\ndependencies = ["gaanim"]\n';
const POINTS = "x,y\n0,0\n1,0.8\n2,1.1\n3,0.9\n";
// A long scene: the recording takes long enough to show its progress.
const LONG = `from gaanim import BLUE, GOLD, Scene

scene = Scene(frame=(16, 9), theme="technical")
cuadros = [
    scene.geometry.rect(0.3, 0.3).fill(BLUE if (i + j) % 2 else GOLD).move_to(-7 + i * 0.5, -4 + j * 0.5)
    for i in range(29)
    for j in range(17)
]
scene.play([cuadro.animate.shift_by(0.5, 0.25).duration(120) for cuadro in cuadros])
scene.render()
`;

const CRC = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(bytes) {
  let c = 0xffffffff;
  for (const byte of bytes) c = CRC[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

// A ZIP archive of `files` ({ path: bytes }), stored without compression.
function zip(files) {
  const locals = [];
  const centrals = [];
  let offset = 0;
  for (const [path, data] of Object.entries(files)) {
    const name = Buffer.from(path, "utf8");
    const bytes = Buffer.from(data);
    const crc = crc32(bytes);
    const local = Buffer.alloc(30);
    local.writeUInt32LE(0x04034b50, 0);
    local.writeUInt16LE(20, 4);
    local.writeUInt16LE(0x0800, 6);
    local.writeUInt32LE(crc, 14);
    local.writeUInt32LE(bytes.length, 18);
    local.writeUInt32LE(bytes.length, 22);
    local.writeUInt16LE(name.length, 26);
    const central = Buffer.alloc(46);
    central.writeUInt32LE(0x02014b50, 0);
    central.writeUInt16LE(20, 4);
    central.writeUInt16LE(20, 6);
    central.writeUInt16LE(0x0800, 8);
    central.writeUInt32LE(crc, 16);
    central.writeUInt32LE(bytes.length, 20);
    central.writeUInt32LE(bytes.length, 24);
    central.writeUInt16LE(name.length, 28);
    central.writeUInt32LE(offset, 42);
    locals.push(local, name, bytes);
    centrals.push(central, name);
    offset += 30 + name.length + bytes.length;
  }
  const directory = Buffer.concat(centrals);
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(Object.keys(files).length, 8);
  end.writeUInt16LE(Object.keys(files).length, 10);
  end.writeUInt32LE(directory.length, 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...locals, directory, end]);
}

// A short tone, so that the project has some audio.
function tone() {
  const rate = 22050;
  const samples = rate / 2;
  const data = Buffer.alloc(44 + samples * 2);
  data.write("RIFF", 0);
  data.writeUInt32LE(36 + samples * 2, 4);
  data.write("WAVEfmt ", 8);
  data.writeUInt32LE(16, 16);
  data.writeUInt16LE(1, 20);
  data.writeUInt16LE(1, 22);
  data.writeUInt32LE(rate, 24);
  data.writeUInt32LE(rate * 2, 28);
  data.writeUInt16LE(2, 32);
  data.writeUInt16LE(16, 34);
  data.write("data", 36);
  data.writeUInt32LE(samples * 2, 40);
  for (let i = 0; i < samples; i++) data.writeInt16LE(Math.round(Math.sin((2 * Math.PI * 523 * i) / rate) * 9000), 44 + i * 2);
  return data;
}

// ---------------------------------------------------------------------------
// The browser, through the DevTools protocol
// ---------------------------------------------------------------------------

const work = mkdtempSync(join(tmpdir(), "gaanim-capture-"));
const profile = join(work, "profile");
const downloads = join(work, "downloads");
mkdirSync(downloads);
const archive = join(work, "Mi animación.zip");
writeFileSync(
  archive,
  zip({
    "mi-animacion/main.py": MAIN,
    "mi-animacion/lib/formas.py": FORMAS,
    "mi-animacion/notas.md": NOTES,
    // As `gaanim init` writes it: the packages panel adds to its list.
    "mi-animacion/pyproject.toml": PYPROJECT,
    "mi-animacion/datos/puntos.csv": POINTS,
    "mi-animacion/assets/fonts/VictorMono.ttf": readFileSync(join(ROOT, "docs", "fixtures", "assets", "fonts", "VictorMono-VariableFont.ttf")),
    "mi-animacion/assets/imagenes/icono.png": readFileSync(join(ROOT, "docs", "assets", "brand", "gaanim-icon-512.png")),
    "mi-animacion/assets/audio/tono.wav": tone(),
  }),
);

const browser = spawn(
  BROWSER,
  [
    "--headless=new",
    `--user-data-dir=${profile}`,
    "--remote-debugging-port=0",
    "--enable-unsafe-webgpu",
    "--ignore-gpu-blocklist",
    "--no-first-run",
    "--no-default-browser-check",
    "--disable-extensions",
    "--hide-scrollbars",
    `--window-size=${WIDTH},${HEIGHT}`,
    "about:blank",
  ],
  { stdio: "ignore" },
);

let port = null;
for (let tries = 0; tries < 100 && !port; tries++) {
  await sleep(100);
  const file = join(profile, "DevToolsActivePort");
  if (existsSync(file)) port = readFileSync(file, "utf8").split("\n")[0].trim() || null;
}
if (!port) throw new Error("the browser did not open its DevTools port");
const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find((target) => target.type === "page");
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((done, fail) => {
  socket.onopen = done;
  socket.onerror = fail;
});
let lastId = 0;
const pending = new Map();
const listeners = new Set();
socket.onmessage = ({ data }) => {
  const message = JSON.parse(data);
  if (message.id) {
    const entry = pending.get(message.id);
    pending.delete(message.id);
    if (message.error) entry.reject(new Error(`${entry.method}: ${message.error.message}`));
    else entry.resolve(message.result);
  } else {
    for (const listener of listeners) listener(message);
  }
};
function send(method, params = {}) {
  const id = ++lastId;
  socket.send(JSON.stringify({ id, method, params }));
  return new Promise((resolve_, reject) => pending.set(id, { resolve: resolve_, reject, method }));
}

async function evaluate(expression) {
  const { result, exceptionDetails } = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (exceptionDetails) throw new Error(`${expression.slice(0, 80)}: ${exceptionDetails.exception?.description ?? exceptionDetails.text}`);
  return result.value;
}

async function until(expression, { timeout = 60000, what = expression } = {}) {
  const started = Date.now();
  while (Date.now() - started < timeout) {
    if (await evaluate(expression).catch(() => false)) return;
    await sleep(200);
  }
  throw new Error(`timed out waiting for ${what}`);
}

const click = (selector) => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`);
const consoleText = () => evaluate(`document.getElementById("console").innerText`);
const doneRuns = async () => ((await consoleText()).match(/✓ Escena lista/g) ?? []).length;

async function waitRun(before) {
  await until(`document.getElementById("console").innerText.split("✓ Escena lista").length - 1 > ${before}`, { what: "a run", timeout: 120000 });
  // The player opens it and draws a few frames.
  await sleep(2500);
}

// The model of `path` in the editor, to type into it as a person would.
async function setFile(path, text) {
  await evaluate(`(() => {
    const model = monaco.editor.getModels().find((m) => m.uri.path === "/proyecto/${path}");
    model.pushEditOperations([], [{ range: model.getFullModelRange(), text: ${JSON.stringify(text)} }], () => null);
  })()`);
}

async function rectOf(selector, pad = 0) {
  const rect = await evaluate(`JSON.stringify(document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect())`);
  const { x, y, width, height } = JSON.parse(rect);
  const left = Math.max(0, x - pad);
  const top = Math.max(0, y - pad);
  return { x: left, y: top, width: Math.min(WIDTH - left, width + pad * 2), height: Math.min(HEIGHT - top, height + pad * 2), scale: 1 };
}

mkdirSync(OUT, { recursive: true });
async function shot(name, clip = null) {
  const { data } = await send("Page.captureScreenshot", { format: "webp", quality: 88, ...(clip ? { clip } : {}) });
  const path = join(OUT, `${name}.webp`);
  writeFileSync(path, Buffer.from(data, "base64"));
  console.log(`${name}.webp  ${(statSync(path).size / 1024).toFixed(0)} KB`);
}

try {
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Emulation.setDeviceMetricsOverride", { width: WIDTH, height: HEIGHT, deviceScaleFactor: SCALE, mobile: false });
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] });
  await send("Browser.setDownloadBehavior", { behavior: "allow", downloadPath: downloads }).catch(() =>
    send("Page.setDownloadBehavior", { behavior: "allow", downloadPath: downloads }),
  );
  const opened = Date.now();
  await send("Page.navigate", { url: URL_ });
  await until(`location.href.startsWith(${JSON.stringify(URL_)}) && document.readyState !== "loading"`, { what: "the page" });
  if (!(await evaluate(`(async () => Boolean(navigator.gpu && (await navigator.gpu.requestAdapter())))()`))) {
    throw new Error("this browser has no WebGPU adapter in headless mode");
  }
  await until(`document.getElementById("console")?.innerText.includes("✓ Escena lista")`, { what: "the first run", timeout: 180000 });
  console.log(`first scene ${((Date.now() - opened) / 1000).toFixed(1)} s after opening the page`);

  // A project from a ZIP, its folders open and its entry point in the editor.
  let runs = await doneRuns();
  const { root } = await send("DOM.getDocument");
  const { nodeId } = await send("DOM.querySelector", { nodeId: root.nodeId, selector: "#zip-input" });
  await send("DOM.setFileInputFiles", { nodeId, files: [archive] });
  await waitRun(runs);
  for (const folder of ["assets", "assets/fonts", "lib"]) {
    await click(`.row[data-path="${folder}"]`);
    await sleep(150);
  }
  await click(`.row[data-path="main.py"]`);
  await evaluate(`document.getElementById("clear-console").click()`);
  runs = 0;
  await click("#run");
  await waitRun(runs);
  await shot("interfaz");

  // An uploaded font, as the explorer previews it.
  await click(`.row[data-path="assets/fonts/VictorMono.ttf"]`);
  await sleep(1200);
  const explorer = await rectOf("#explorer");
  const editorPane = await rectOf(".editor-pane");
  await shot("fuente", { ...explorer, width: explorer.width + editorPane.width, height: Math.min(explorer.height, 560) });

  // The projects menu.
  await click("#project-switch");
  await sleep(400);
  const menu = await rectOf("#context-menu");
  await shot("proyectos", { x: 0, y: explorer.y, width: Math.max(menu.x + menu.width + 24, explorer.width + 40), height: menu.y - explorer.y + menu.height + 24, scale: 1 });
  await evaluate(`document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }))`);

  // An error inside a module: its file opens at the line.
  await click(`.row[data-path="lib/formas.py"]`);
  await sleep(300);
  await setFile("lib/formas.py", BROKEN);
  await evaluate(`document.getElementById("clear-console").click()`);
  await click("#run");
  await until(`document.getElementById("status").textContent.includes("falló")`, { what: "the failure" });
  await sleep(800);
  await shot("error");
  await setFile("lib/formas.py", FORMAS);

  // The packages panel: one package of Pyodide and one of PyPI go into the
  // project's pyproject.toml and install.
  await click("#tab-packages");
  const typePackage = (name) =>
    evaluate(`(() => {
      const query = document.getElementById("package-query");
      query.focus();
      query.value = ${JSON.stringify(name)};
      query.dispatchEvent(new Event("input", { bubbles: true }));
    })()`);
  for (const [index, name] of ["numpy", "webcolors"].entries()) {
    await typePackage(name);
    await until(`!document.getElementById("package-suggestions").hidden`, { what: `the suggestions for ${name}` });
    await evaluate(`document.getElementById("package-form").requestSubmit()`);
    await until(`document.querySelectorAll(".package-status.ok").length > ${index}`, { what: `${name} installed`, timeout: 120000 });
  }
  await sleep(300);
  await shot("paquetes", await rectOf(".panel"));
  await evaluate(`(() => {
    const query = document.getElementById("package-query");
    query.value = "";
    query.blur();
  })()`);
  await click("#tab-console");

  // A long recording: its progress and time, then Stop.
  await click(`.row[data-path="main.py"]`);
  await sleep(300);
  await setFile("main.py", LONG);
  await click("#run");
  await until(`/% · ([2-9]|\\d\\d) s/.test(document.getElementById("player-overlay-text").textContent)`, { what: "the recording's progress", timeout: 120000 });
  await shot("grabando", await rectOf("#player-wrap"));
  if (await evaluate(`document.getElementById("stop").hidden`)) throw new Error("the long recording ended before Stop");
  const stopped = Date.now();
  await click("#stop");
  await until(`document.getElementById("console").innerText.includes("■ Detenido")`, { what: "the stop" });
  await until(`!document.getElementById("run").disabled`, { what: "Python again", timeout: 120000 });
  // With a spare Python warmed during the run, the next run can start at once.
  console.log(`stop: Python ready ${((Date.now() - stopped) / 1000).toFixed(1)} s after Stop`);
  await setFile("main.py", MAIN);
  await until(`!document.getElementById("run").disabled`, { what: "Python again", timeout: 120000 });
  runs = await doneRuns();
  await click("#run");
  await waitRun(runs);

  // The video dialog; its MP4 must download.
  await click("#download-toggle");
  await click("#download-video");
  await sleep(1500);
  await shot("video", await rectOf("#video-dialog", 16));
  await evaluate(`document.getElementById("video-height").value = "720"`);
  await click("#video-start");
  await until(`document.getElementById("video-status").textContent.includes("descargado")`, { what: "the video export", timeout: 180000 });
  const status = await evaluate(`document.getElementById("video-status").textContent`);
  await sleep(1000);
  const videos = readdirSync(downloads).filter((name) => name.endsWith(".mp4"));
  if (!videos.length) throw new Error(`no MP4 downloaded (${status})`);
  console.log(`video: ${status} → ${videos[0]}, ${(statSync(join(downloads, videos[0])).size / 1024).toFixed(0)} KB`);
  await click("#video-close");

  // Insertar, with its preview.
  await click("#embed");
  await sleep(4000);
  await shot("insertar");
  // Its page with everything inside must download.
  await click("#embed-html");
  let pages = [];
  for (let tries = 0; tries < 100 && !pages.length; tries++) {
    await sleep(300);
    pages = readdirSync(downloads).filter((name) => name.endsWith(".html"));
  }
  if (!pages.length) throw new Error(`no HTML page downloaded (${await evaluate(`document.getElementById("toast").textContent`)})`);
  await sleep(500);
  console.log(`page: ${pages[0]}, ${(statSync(join(downloads, pages[0])).size / 1e6).toFixed(1)} MB`);
  await click("#embed-close");

  // The light theme, after a clean run.
  await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "light" }] });
  await click(`.row[data-path="main.py"]`);
  await evaluate(`document.getElementById("clear-console").click()`);
  await click("#run");
  await waitRun(0);
  await shot("claro");
} finally {
  socket.close();
  browser.kill();
  await sleep(500);
  rmSync(work, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}
