// Gaanim Playground: write a scene in Python, run it in the browser, watch it
// in the player.
//
// - The project is a tree of files, as in a code editor: `main.py`, which
//   runs, the modules it imports and its assets (images, music, fonts, data)
//   in folders. It lives in this browser (IndexedDB); the explorer creates,
//   uploads (files or whole folders), renames, moves and deletes them.
// - The editor is Monaco, with a tab per open file. Completions, signatures,
//   hovers and syntax errors come from Jedi over the gaanim stubs and the
//   project's own modules, in a worker (language.js).
// - Scripts run in another worker (runner.js): Pyodide with the gaanim wheel
//   built for it. A run writes the project, runs `main.py` and records the
//   scene into a .gaanim bundle.
// - The embed player (`reproductor/insertar/`), the light one a blog gets,
//   plays that bundle; one that needs the GPU passes it leaves out
//   (post-processing, shader backgrounds) plays in the web player (`?embed`).
// - Insertar prepares the recording for a blog or website: the file to
//   publish there and the <iframe> of the embed player that plays it, with a
//   preview of that player.
// Nothing leaves the browser: the page is static files.

import { FORMATS, evenSize, exportVideo, supportedCodec } from "./video.js";

const MONACO = "https://cdn.jsdelivr.net/npm/monaco-editor@0.57.0/min";
const KEYS = {
  // main.py before projects had files of their own; read once, to migrate.
  code: "gaanim-playground-code",
  fps: "gaanim-playground-fps",
  layout: "gaanim-playground-layout",
  embed: "gaanim-playground-embed",
  tabs: "gaanim-playground-tabs",
  expanded: "gaanim-playground-expanded",
  explorer: "gaanim-playground-explorer",
  projects: "gaanim-playground-projects",
  current: "gaanim-playground-current",
  live: "gaanim-playground-live",
  video: "gaanim-playground-video",
};
// The file that runs.
const ENTRY = "main.py";
// Files edited as text; any other file is an asset, kept as bytes.
const TEXT_EXTENSIONS = new Set([
  "py", "pyi", "txt", "md", "json", "csv", "tsv", "toml", "yaml", "yml", "typ", "wgsl", "glsl", "xml", "html", "css", "js",
]);
const MAX_FILE = 80 * 1024 * 1024;

// State the handlers below share. Declared first: editor and worker events can
// fire while the module still awaits its later steps.
let syntaxTimer = 0;
let languageTimer = 0;
let languageReady = false;
// Live mode: the scene runs a moment after typing stops.
let live = false;
let liveTimer = 0;
// The project: files by path ({ path, text } or { path, bytes, type, size,
// modified }) and folders, empty ones included.
const project = new Map();
const folders = new Set();

const $ = (id) => document.getElementById(id);
const ui = {
  run: $("run"),
  stop: $("stop"),
  share: $("share"),
  fps: $("fps"),
  downloadToggle: $("download-toggle"),
  downloadMenu: $("download-menu"),
  downloadProject: $("download-project"),
  downloadVideo: $("download-video"),
  live: $("live"),
  projectSwitch: $("project-switch"),
  projectName: $("project-name"),
  zipInput: $("zip-input"),
  videoDialog: $("video-dialog"),
  videoClose: $("video-close"),
  videoFormat: $("video-format"),
  videoHeight: $("video-height"),
  videoMeta: $("video-meta"),
  videoWarning: $("video-warning"),
  videoProgress: $("video-progress"),
  videoStatus: $("video-status"),
  videoCancel: $("video-cancel"),
  videoStart: $("video-start"),
  downloadBundle: $("download-bundle"),
  explorer: $("explorer"),
  explorerToggle: $("explorer-toggle"),
  tree: $("tree"),
  newFile: $("new-file"),
  newFolder: $("new-folder"),
  uploadFiles: $("upload-files"),
  uploadFolder: $("upload-folder"),
  fileInput: $("file-input"),
  folderInput: $("folder-input"),
  editorTabs: $("editor-tabs"),
  editor: $("editor"),
  assetView: $("asset-view"),
  cursor: $("cursor"),
  contextMenu: $("context-menu"),
  player: $("player"),
  overlay: $("player-overlay"),
  overlayText: $("player-overlay-text"),
  overlayHint: $("player-overlay-hint"),
  console: $("console"),
  clearConsole: $("clear-console"),
  status: $("status"),
  statusDot: $("status-dot"),
  progress: $("progress"),
  versions: $("versions"),
  toast: $("toast"),
  dropping: $("dropping"),
  embed: $("embed"),
  theme: $("theme"),
  embedDialog: $("embed-dialog"),
  embedClose: $("embed-close"),
  embedFrame: $("embed-frame"),
  embedPreview: $("embed-preview"),
  embedMeta: $("embed-meta"),
  embedName: $("embed-name"),
  embedDownload: $("embed-download"),
  embedUrl: $("embed-url"),
  embedTest: $("embed-test"),
  embedTestWrap: $("embed-test-wrap"),
  embedLoop: $("embed-loop"),
  embedAutoplay: $("embed-autoplay"),
  embedControls: $("embed-controls"),
  embedCode: $("embed-code"),
  embedCopy: $("embed-copy"),
  embedLocal: $("embed-local"),
};

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

function load(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // Private windows may refuse storage; the page still works.
  }
}

function loadJson(key, fallback) {
  try {
    return JSON.parse(load(key) ?? "null") ?? fallback;
  } catch {
    return fallback;
  }
}

function setStatus(text, state = "busy") {
  ui.status.textContent = text;
  ui.statusDot.className = `dot ${state}`;
}

let toastTimer = 0;
// Show `text`, with a button that runs `action.run` when given.
function toast(text, ms = 2600, action = null) {
  ui.toast.replaceChildren(text);
  if (action) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "toast-action";
    button.textContent = action.label;
    button.addEventListener("click", () => {
      ui.toast.hidden = true;
      action.run();
    });
    ui.toast.append(" ", button);
  }
  ui.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast.hidden = true), action ? Math.max(ms, 6000) : ms);
}

function downloadBlob(name, data, type = "application/octet-stream") {
  const url = URL.createObjectURL(new Blob([data], { type }));
  const anchor = Object.assign(document.createElement("a"), { href: url, download: name });
  document.body.append(anchor);
  anchor.click();
  anchor.remove();
  setTimeout(() => URL.revokeObjectURL(url), 60000);
}

function formatSize(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

function baseName(path) {
  return path.slice(path.lastIndexOf("/") + 1);
}

function parentOf(path) {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}

function join(folder, name) {
  return folder ? `${folder}/${name}` : name;
}

function extension(path) {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : "";
}

function isText(path) {
  return TEXT_EXTENSIONS.has(extension(path));
}

// What a file holds, for its icon, its preview and the code that uses it.
function kindOf(path) {
  const ext = extension(path);
  if (ext === "py" || ext === "pyi") return "python";
  if (ext === "svg") return "svg";
  if (["png", "jpg", "jpeg", "webp", "gif", "bmp", "avif"].includes(ext)) return "image";
  if (["mp3", "wav", "ogg", "oga", "m4a", "aac", "flac", "opus", "weba"].includes(ext)) return "audio";
  if (["ttf", "otf", "ttc", "otc", "woff", "woff2"].includes(ext)) return "font";
  if (["json", "csv", "tsv", "toml", "yaml", "yml", "lottie"].includes(ext)) return "data";
  if (["mp4", "webm", "mov", "mkv"].includes(ext)) return "video";
  return isText(path) ? "text" : "file";
}

// Pictograms of the explorer, the tabs and the asset preview, on a 16-unit
// grid as the brand draws: square corners and joins, lines in the type's
// color, accents in violet and gold, and the symbol's three frames (haze,
// violet, gold) for a recording or an animation.
const ICONS = {
  folder: '<path class="s" d="M1.75 3.75h4.5l1.5 1.5h6.5v7.5H1.75z"/>',
  "folder-open": '<path class="s" d="M1.75 12.75v-9h4.5l1.5 1.5h5.5v1.5"/><path class="s" d="M1.75 12.75l1.75-5h11l-1.75 5z"/>',
  python:
    '<path class="v" d="M2 2h7v5H5v2.5H2z"/><path class="g" d="M14 14H7V9h4V6.5h3z"/>' +
    '<rect class="eye" x="3.5" y="3.5" width="1.5" height="1.5"/><rect class="eye" x="11" y="11" width="1.5" height="1.5"/>',
  markdown:
    '<rect class="s" x="1.75" y="3.75" width="12.5" height="8.5"/><path class="s" d="M4.25 10V6.25L6 8.25l1.75-2V10"/>' +
    '<path class="gs" d="M11 6v3.75M9.5 8.5L11 10l1.5-1.5"/>',
  json: '<path class="s" d="M5.75 2.75h-1.5v4L3 8l1.25 1.25v4h1.5M10.25 2.75h1.5v4L13 8l-1.25 1.25v4h-1.5"/>',
  table:
    '<rect class="o" x="1.75" y="2.75" width="12.5" height="3.5"/><rect class="s" x="1.75" y="2.75" width="12.5" height="10.5"/>' +
    '<path class="s" d="M1.75 6.25h12.5M1.75 9.75h12.5M6 2.75v10.5M10 2.75v10.5"/>',
  config:
    '<path class="s" d="M1.75 4.5h12.5M1.75 8h12.5M1.75 11.5h12.5"/><rect class="g" x="8.5" y="2.75" width="3.5" height="3.5"/>' +
    '<rect class="g" x="3.5" y="6.25" width="3.5" height="3.5"/><rect class="g" x="6.5" y="9.75" width="3.5" height="3.5"/>',
  lottie:
    '<path class="s" d="M2.5 12.5C5 12.5 5.5 3.5 8 3.5s3 9 5.5 9"/><rect class="h" x="1" y="11" width="3" height="3"/>' +
    '<rect class="v" x="6.5" y="2" width="3" height="3"/><rect class="g" x="12" y="11" width="3" height="3"/>',
  image:
    '<rect class="s" x="1.75" y="2.75" width="12.5" height="10.5"/><path class="v" d="M2.5 12.5l3.75-5 2.75 3.5 1.5-1.75 3 3.25z"/>' +
    '<rect class="g" x="10" y="4.75" width="2.5" height="2.5"/>',
  vector:
    '<path class="s" d="M3 13C3.5 6.5 12.5 9.5 13 3"/><rect class="g" x="1.25" y="11.25" width="3.5" height="3.5"/>' +
    '<rect class="g" x="11.25" y="1.25" width="3.5" height="3.5"/>',
  audio: '<path class="s" d="M2.75 6.75v2.5M5.25 4.25v7.5M10.25 5v6M12.75 6.75v2.5"/><path class="gs" d="M7.75 2.5v11"/>',
  video: '<rect class="s" x="1.75" y="3.25" width="12.5" height="9.5"/><path class="g" d="M6.25 5.5v5L10.5 8z"/>',
  font: '<path class="s" d="M3 13.75L8 2.25l5 11.5M4.75 9.75h6.5"/>',
  text: '<path class="s" d="M3.25 1.75h6.25l3.25 3.25v9.25h-9.5z"/><path class="s" d="M9.5 1.75V5h3.25M5.5 8h5M5.5 11h5"/>',
  code: '<path class="s" d="M5 4.5L1.75 8 5 11.5M11 4.5L14.25 8 11 11.5"/><path class="gs" d="M9.25 3l-2.5 10"/>',
  math: '<path class="s" d="M12.25 3.25h-8.5L8.5 8l-4.75 4.75h8.5"/>',
  gaanim:
    '<rect class="h" x="0.5" y="6" width="4" height="4"/><rect class="v" x="6" y="6" width="4" height="4"/>' +
    '<rect class="g" x="11.5" y="6" width="4" height="4"/>',
  file: '<path class="s" d="M3.25 1.75h6.25l3.25 3.25v9.25h-9.5z"/><path class="s" d="M9.5 1.75V5h3.25"/>',
};

// The pictogram of `path`, finer than its kind: Markdown, tables, settings,
// code and math each have their own.
function iconOf(path) {
  const ext = extension(path);
  if (ext === "gaanim") return "gaanim";
  if (ext === "md") return "markdown";
  if (ext === "json") return "json";
  if (ext === "lottie") return "lottie";
  if (ext === "csv" || ext === "tsv") return "table";
  if (["toml", "yaml", "yml"].includes(ext)) return "config";
  if (["typ", "tex"].includes(ext)) return "math";
  if (["html", "css", "js", "xml", "wgsl", "glsl"].includes(ext)) return "code";
  const kind = kindOf(path);
  if (kind === "svg") return "vector";
  return ICONS[kind] ? kind : "file";
}

function fileIcon(name, { big = false } = {}) {
  const icon = document.createElement("span");
  icon.className = `file-icon ${name}${big ? " big" : ""}`;
  icon.setAttribute("aria-hidden", "true");
  icon.innerHTML = `<svg viewBox="0 0 16 16">${ICONS[name] ?? ICONS.file}</svg>`;
  return icon;
}

// A path as the project keeps it: forward slashes, no empty, `.` or `..`
// parts, and no characters that paths reject.
function cleanPath(path) {
  return path
    .split(/[\\/]+/)
    .map((part) => part.replace(/[\u0000-\u001f"<>|*?:]/g, "_").trim())
    .filter((part) => part && part !== "." && part !== "..")
    .join("/");
}

// Every folder of the project: the explicit ones and those its files sit in.
function allFolders() {
  const all = new Set(folders);
  for (const path of project.keys()) {
    for (let folder = parentOf(path); folder; folder = parentOf(folder)) all.add(folder);
  }
  return all;
}

function exists(path) {
  return project.has(path) || allFolders().has(path);
}

// Unfold the folders that hold `path`, so it shows in the explorer.
function reveal(path) {
  for (let folder = parentOf(path); folder; folder = parentOf(folder)) expanded.add(folder);
  saveExpanded();
}

// ---------------------------------------------------------------------------
// Console
// ---------------------------------------------------------------------------

function consoleEmpty() {
  ui.console.innerHTML = '<p class="empty">La salida de <code>print</code> y los errores aparecen aquí.</p>';
}

// The fonts the browser has without uploading any (typst-assets).
const BUILTIN_FONTS = "New Computer Modern, Libertinus Serif y DejaVu Sans Mono";
// Whether this run already said how to add a font.
let fontHinted = false;

function fontHint(text) {
  const match = /unknown font family: ([^\n"]+)|Font family not found/i.exec(text);
  if (!match || fontHinted) return;
  fontHinted = true;
  const family = match[1]?.trim() || "Familia";
  log(
    `Esa fuente no está en el navegador, que solo trae ${BUILTIN_FONTS}. Sube su archivo (.ttf u .otf) a assets/fonts y nómbrala en el tema: Theme(font_dir="assets/fonts", fonts={"text": "${family}"}).`,
    "hint",
  );
}

// `action` ({ label, run }) adds a link after the text.
function log(text, kind = "", action = null) {
  ui.console.querySelector(".empty")?.remove();
  const line = document.createElement("div");
  line.className = `line ${kind}`;
  // `File "lib/util.py", line N` jumps to that file of the project.
  const pattern = /File "([^"]+)", line (\d+)/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    if (!project.has(match[1])) continue;
    line.append(text.slice(last, match.index));
    const link = document.createElement("a");
    link.textContent = match[0];
    const [path, number] = [match[1], Number(match[2])];
    link.addEventListener("click", () => openFile(path, { line: number }));
    line.append(link);
    last = match.index + match[0].length;
  }
  line.append(text.slice(last));
  if (action) {
    const link = document.createElement("a");
    link.textContent = action.label;
    link.className = "action";
    link.addEventListener("click", () => action.run(), { once: true });
    line.append(" ", link);
  }
  const atBottom = ui.console.scrollHeight - ui.console.scrollTop - ui.console.clientHeight < 40;
  ui.console.append(line);
  if (atBottom) ui.console.scrollTop = ui.console.scrollHeight;
  if (kind !== "hint") fontHint(text);
}

ui.clearConsole.addEventListener("click", consoleEmpty);
consoleEmpty();

// ---------------------------------------------------------------------------
// Configuration written by scripts/build_playground.py
// ---------------------------------------------------------------------------

// Always revalidated: it names the current wheel and player.
const config = await fetch("config.json", { cache: "no-cache" })
  .then((response) => response.json())
  .catch(() => null);
if (!config) {
  setStatus("No se encontró config.json: construye el playground con scripts/build_playground.py.", "error");
  throw new Error("config.json missing");
}
config.base = new URL(".", location.href).href;

// ---------------------------------------------------------------------------
// Shared links: the project's text files travel compressed in the fragment.
// ---------------------------------------------------------------------------

function toBase64Url(bytes) {
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(text) {
  const binary = atob(text.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(binary, (char) => char.charCodeAt(0));
}

async function compress(text) {
  const stream = new Blob([text]).stream().pipeThrough(new CompressionStream("deflate-raw"));
  return toBase64Url(new Uint8Array(await new Response(stream).arrayBuffer()));
}

async function decompress(text) {
  const stream = new Blob([fromBase64Url(text)]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
  return new Response(stream).text();
}

// The text files of a shared link, by path: `#p=` holds a project, `#c=` (the
// links before projects) a main.py.
async function sharedFiles() {
  const params = new URLSearchParams(location.hash.slice(1));
  try {
    if (params.get("p")) {
      const shared = JSON.parse(await decompress(params.get("p")));
      const files = {};
      for (const [path, text] of Object.entries(shared.files ?? {})) {
        const clean = cleanPath(path);
        if (clean && typeof text === "string") files[clean] = text;
      }
      return files;
    }
    if (params.get("c")) return { [ENTRY]: await decompress(params.get("c")) };
  } catch {
    toast("El enlace no contiene código válido.");
  }
  return null;
}

// ---------------------------------------------------------------------------
// Examples
// ---------------------------------------------------------------------------

// The gallery's scenes, which the projects menu opens as new projects.
const examples = await fetch(config.examples)
  .then((response) => response.json())
  .catch(() => []);

const FALLBACK_CODE = `from gaanim import Easing, BLACK, BLUE, GOLD, WHITE, Scene

scene = Scene(frame=(16, 9), background=BLACK, margin=0.6)

circle = scene.geometry.circle(1.2).fill(BLUE).stroke(WHITE, 0.05)
title = scene.text("Hola, Gaanim", role="title").fill(GOLD).move_to(0, 2.25)

scene.play([
    circle.animate.create().duration(0.8),
    title.animate.write().duration(0.6),
])
scene.play([circle.animate.shift_by(3, 0).duration(1.0).easing(Easing.SMOOTH)])
scene.render()
`;

// ---------------------------------------------------------------------------
// Storage: the projects in IndexedDB
// ---------------------------------------------------------------------------

// Each project keeps its files by [project, path] in "entries" (folders as
// records with `folder: true`); the projects' names and the open one are in
// localStorage. Version 1 kept uploads by name in "files", version 2 one
// project by path in "project": both become the first project.
const DEFAULT_PROJECT = "default";
const database = await new Promise((resolve) => {
  try {
    const request = indexedDB.open("gaanim-playground", 3);
    request.onupgradeneeded = (event) => {
      const db = request.result;
      const tx = request.transaction;
      const entries = db.objectStoreNames.contains("entries")
        ? tx.objectStore("entries")
        : db.createObjectStore("entries", { keyPath: ["project", "path"] });
      const move = (name, convert) => {
        if (!db.objectStoreNames.contains(name)) return;
        tx.objectStore(name).openCursor().onsuccess = ({ target }) => {
          const cursor = target.result;
          if (!cursor) return;
          const record = convert(cursor.value);
          if (record) entries.put({ ...record, project: DEFAULT_PROJECT });
          cursor.continue();
        };
      };
      if (event.oldVersion === 2) move("project", (record) => record);
      if (event.oldVersion === 1) {
        move("files", (record) => {
          const path = cleanPath(record.name ?? "");
          return path ? { path, bytes: record.bytes, type: record.type, size: record.size, modified: record.modified } : null;
        });
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => resolve(null);
  } catch {
    resolve(null);
  }
});

function transaction(mode, work) {
  if (!database) return Promise.resolve(null);
  return new Promise((resolve) => {
    try {
      const tx = database.transaction("entries", mode);
      const result = work(tx.objectStore("entries"));
      tx.oncomplete = () => resolve(result?.result ?? null);
      tx.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

// Every entry of project `id`.
function projectRange(id) {
  return IDBKeyRange.bound([id, ""], [id, "\uffff"]);
}

// The projects, { id, name }, in the order they were made, and the open one.
let projects = loadJson(KEYS.projects, []).filter((entry) => entry?.id && entry?.name);
if (!projects.length) projects = [{ id: DEFAULT_PROJECT, name: "Mi proyecto" }];
let currentProject = projects.some((entry) => entry.id === load(KEYS.current)) ? load(KEYS.current) : projects[0].id;

function saveProjects() {
  store(KEYS.projects, JSON.stringify(projects));
  store(KEYS.current, currentProject);
}

function currentName() {
  return projects.find((entry) => entry.id === currentProject)?.name ?? "Proyecto";
}

function newProjectId() {
  return `p${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

// What each project remembers apart (its tabs, its open folders).
function projectKey(key) {
  return currentProject === DEFAULT_PROJECT ? key : `${key}:${currentProject}`;
}

const saveTimers = new Map();
// Keep `path` as it is now; text edits wait a moment, so typing does not
// write on every key.
function persist(path, { soon = false } = {}) {
  clearTimeout(saveTimers.get(path));
  const id = currentProject;
  const write = () => {
    saveTimers.delete(path);
    const file = id === currentProject ? project.get(path) : null;
    if (file) transaction("readwrite", (s) => s.put({ ...file, project: id }));
    else if (id === currentProject && folders.has(path)) transaction("readwrite", (s) => s.put({ project: id, path, folder: true }));
    else if (id === currentProject) transaction("readwrite", (s) => s.delete([id, path]));
  };
  if (soon) saveTimers.set(path, setTimeout(write, 300));
  else write();
}

function flushSaves() {
  for (const [path, timer] of saveTimers) {
    clearTimeout(timer);
    saveTimers.delete(path);
    persist(path);
  }
}

// Load project `id` into `project` and `folders`.
async function loadProject(id) {
  project.clear();
  folders.clear();
  for (const record of (await transaction("readonly", (s) => s.getAll(projectRange(id)))) ?? []) {
    const path = cleanPath(record.path ?? "");
    if (!path) continue;
    if (record.folder) {
      folders.add(path);
    } else if (typeof record.text === "string") {
      project.set(path, { path, text: record.text });
    } else if (record.bytes && isText(path)) {
      // A text file uploaded before text files were edited here.
      project.set(path, { path, text: new TextDecoder().decode(record.bytes) });
      persist(path);
    } else if (record.bytes) {
      const file = { path, bytes: record.bytes, type: record.type ?? "", size: record.size ?? record.bytes.byteLength, modified: record.modified ?? 0 };
      if (kindOf(path) === "font") file.family = fontFamily(record.bytes);
      project.set(path, file);
    }
  }
}

// A project without main.py gets one: the first project from the code of
// the playground before projects, any other from the first example.
function ensureEntry() {
  if (project.has(ENTRY)) return;
  const legacy = currentProject === DEFAULT_PROJECT ? load(KEYS.code) : null;
  project.set(ENTRY, { path: ENTRY, text: legacy ?? examples[0]?.code ?? FALLBACK_CODE });
  persist(ENTRY);
}

await loadProject(currentProject);
// A shared link opens as a project of its own, so it never replaces one;
// the link of the open project (reloading after sharing) opens it.
const shared = await sharedFiles();
const sameProject = (files) => Object.entries(files).every(([path, text]) => project.get(path)?.text === text);
if (shared && !sameProject(shared)) {
  currentProject = newProjectId();
  projects.push({ id: currentProject, name: "Proyecto compartido" });
  project.clear();
  folders.clear();
  for (const [path, text] of Object.entries(shared)) {
    project.set(path, { path, text });
    persist(path);
  }
  // Reloading the page must not make it again.
  history.replaceState(null, "", location.pathname + location.search);
}
saveProjects();
ensureEntry();

// ---------------------------------------------------------------------------
// Monaco
// ---------------------------------------------------------------------------

// Monaco's own worker (bracket matching, links) loads from the CDN through a
// same-origin blob, which queues messages until its module has loaded.
window.MonacoEnvironment = {
  getWorker() {
    const source = `
      self.MonacoEnvironment = { baseUrl: "${MONACO}/" };
      const queued = [];
      self.onmessage = (event) => queued.push(event);
      importScripts("${MONACO}/vs/loader.js");
      require.config({ baseUrl: "${MONACO}/" });
      require(["vs/editor/editor.worker"], () => {
        for (const event of queued) self.onmessage(event);
      });
    `;
    return new Worker(URL.createObjectURL(new Blob([source], { type: "text/javascript" })));
  },
};

const monaco = await new Promise((resolve, reject) => {
  window.require.config({ paths: { vs: `${MONACO}/vs` } });
  window.require(["vs/editor/editor.main"], () => resolve(window.monaco), reject);
}).catch((error) => {
  setStatus(`No se pudo cargar el editor: ${error}`, "error");
  throw error;
});

// Python as the editor colors it: Monaco's own tokenizer leaves names,
// calls and attributes in one color, so most of a scene was white. This one
// tells apart calls, classes, constants (GOLD), keyword arguments, `self`,
// attributes, decorators and f-string fields.
monaco.languages.setMonarchTokensProvider("python", {
  defaultToken: "",
  tokenPostfix: ".python",
  keywords: [
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif", "else", "except",
    "finally", "for", "from", "global", "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass",
    "raise", "return", "try", "while", "with", "yield",
  ],
  constants: ["True", "False", "None", "Ellipsis", "NotImplemented", "__name__", "__file__"],
  selves: ["self", "cls"],
  builtins: [
    "abs", "all", "any", "bool", "dict", "dir", "divmod", "enumerate", "filter", "float", "format", "getattr",
    "hasattr", "hash", "id", "input", "int", "isinstance", "issubclass", "iter", "len", "list", "map", "max", "min",
    "next", "object", "open", "ord", "chr", "pow", "print", "property", "range", "repr", "reversed", "round", "set",
    "setattr", "slice", "sorted", "staticmethod", "classmethod", "str", "sum", "super", "tuple", "type", "vars", "zip",
  ],
  ident: /[a-zA-Z_À-￿][\wÀ-￿]*/,
  brackets: [
    { open: "{", close: "}", token: "delimiter.curly" },
    { open: "[", close: "]", token: "delimiter.square" },
    { open: "(", close: ")", token: "delimiter.parenthesis" },
  ],
  tokenizer: {
    root: [{ include: "@code" }, [/[)\]}]/, "@brackets"]],
    // Inside brackets, where `name=` is a keyword argument.
    nested: [
      [/[)\]}]/, { token: "@brackets", next: "@pop" }],
      [/@ident(?=\s*=(?!=))/, { cases: { "@keywords": "keyword", "@default": "variable.parameter" } }],
      { include: "@code" },
    ],
    code: [
      [/\s+/, "white"],
      [/#.*$/, "comment"],
      [/[([{]/, { token: "@brackets", next: "@nested" }],
      [/(?:[rR][fF]|[fF][rR]?)"""/, { token: "string", next: "@fTripleDouble" }],
      [/(?:[rR][fF]|[fF][rR]?)'''/, { token: "string", next: "@fTripleSingle" }],
      [/(?:[rR][fF]|[fF][rR]?)"/, { token: "string", next: "@fDouble" }],
      [/(?:[rR][fF]|[fF][rR]?)'/, { token: "string", next: "@fSingle" }],
      [/(?:[rRbBuU]|[rR][bB]|[bB][rR])?"""/, { token: "string", next: "@tripleDouble" }],
      [/(?:[rRbBuU]|[rR][bB]|[bB][rR])?'''/, { token: "string", next: "@tripleSingle" }],
      [/(?:[rRbBuU]|[rR][bB]|[bB][rR])?"/, { token: "string", next: "@double" }],
      [/(?:[rRbBuU]|[rR][bB]|[bB][rR])?'/, { token: "string", next: "@single" }],
      [/0[xX][\da-fA-F_]+|0[bB][01_]+|0[oO][0-7_]+/, "number"],
      [/(?:\d[\d_]*\.?[\d_]*|\.\d[\d_]*)(?:[eE][+-]?\d[\d_]*)?[jJ]?/, "number"],
      [/@@ident(?:\.@ident)*/, "decorator"],
      [/(def)(\s+)(@ident)/, ["keyword", "white", "function.definition"]],
      [/(class)(\s+)(@ident)/, ["keyword", "white", "type.definition"]],
      [/(\.)(@ident)(?=\s*\()/, ["delimiter", "function.method"]],
      [/(\.)([A-Z][A-Z\d_]+)(?![\wÀ-￿])/, ["delimiter", "constant"]],
      [/(\.)(@ident)/, ["delimiter", "property"]],
      [
        /@ident(?=\s*\()/,
        {
          cases: {
            "@keywords": "keyword",
            "@builtins": "function.builtin",
            "~[A-Z][A-Z\\d_]+": "constant",
            "~[A-Z]\\w*": "type",
            "@default": "function.call",
          },
        },
      ],
      [
        /@ident/,
        {
          cases: {
            "@keywords": "keyword",
            "@constants": "keyword.constant",
            "@selves": "variable.self",
            "@builtins": "function.builtin",
            "~[A-Z][A-Z\\d_]+": "constant",
            "~[A-Z]\\w*": "type",
            "@default": "identifier",
          },
        },
      ],
      [/->|[-+*/%=<>!&|^~@]=?|\*\*=?|\/\/=?|<<=?|>>=?|:=/, "operator"],
      [/[,:;.]/, "delimiter"],
    ],
    double: [[/\\./, "string.escape"], [/"/, { token: "string", next: "@pop" }], [/[^\\"]+/, "string"]],
    single: [[/\\./, "string.escape"], [/'/, { token: "string", next: "@pop" }], [/[^\\']+/, "string"]],
    tripleDouble: [[/\\./, "string.escape"], [/"""/, { token: "string", next: "@pop" }], [/[^\\"]+|"/, "string"]],
    tripleSingle: [[/\\./, "string.escape"], [/'''/, { token: "string", next: "@pop" }], [/[^\\']+|'/, "string"]],
    fDouble: [
      [/\{\{|\}\}|\\./, "string.escape"],
      [/\{[^{}]*\}/, "string.interpolation"],
      [/"/, { token: "string", next: "@pop" }],
      [/[^\\"{}]+|[{}]/, "string"],
    ],
    fSingle: [
      [/\{\{|\}\}|\\./, "string.escape"],
      [/\{[^{}]*\}/, "string.interpolation"],
      [/'/, { token: "string", next: "@pop" }],
      [/[^\\'{}]+|[{}]/, "string"],
    ],
    fTripleDouble: [
      [/\{\{|\}\}|\\./, "string.escape"],
      [/\{[^{}]*\}/, "string.interpolation"],
      [/"""/, { token: "string", next: "@pop" }],
      [/[^\\"{}]+|["{}]/, "string"],
    ],
    fTripleSingle: [
      [/\{\{|\}\}|\\./, "string.escape"],
      [/\{[^{}]*\}/, "string.interpolation"],
      [/'''/, { token: "string", next: "@pop" }],
      [/[^\\'{}]+|['{}]/, "string"],
    ],
  },
});

// The brand's palette (docs/assets/brand/README.md) for the code: violet
// keywords, gold strings and the current frame (the cursor), and around
// them sky calls, lilac classes and peach values, so a scene reads by role.
const SYNTAX = {
  dark: {
    text: "e4e1f2", comment: "77738f", keyword: "a99fff", value: "ff9f7a", string: "ffd166", escape: "ffb347",
    field: "c9c3ff", call: "7cc7ff", type: "e3a6ff", parameter: "bdb6e6", property: "cfcbe6", decorator: "ffc933",
    operator: "b4aed6", delimiter: "8e89a8",
  },
  light: {
    text: "1f1d2b", comment: "8a8799", keyword: "5b4af0", value: "c2410c", string: "a16207", escape: "b45309",
    field: "4636d6", call: "0369a1", type: "a21caf", parameter: "6d5fb3", property: "3f3c56", decorator: "b45309",
    operator: "6b6880", delimiter: "6b6880",
  },
};

function syntaxRules(c) {
  return [
    { token: "", foreground: c.text },
    { token: "identifier", foreground: c.text },
    { token: "comment", foreground: c.comment, fontStyle: "italic" },
    { token: "keyword", foreground: c.keyword },
    { token: "keyword.constant", foreground: c.value },
    { token: "number", foreground: c.value },
    { token: "constant", foreground: c.value },
    { token: "variable.self", foreground: c.value, fontStyle: "italic" },
    { token: "string", foreground: c.string },
    { token: "string.escape", foreground: c.escape },
    { token: "string.interpolation", foreground: c.field },
    { token: "function", foreground: c.call },
    { token: "function.definition", foreground: c.call, fontStyle: "bold" },
    { token: "function.builtin", foreground: c.call, fontStyle: "italic" },
    { token: "type", foreground: c.type },
    { token: "type.definition", foreground: c.type, fontStyle: "bold" },
    { token: "variable.parameter", foreground: c.parameter, fontStyle: "italic" },
    { token: "property", foreground: c.property },
    { token: "decorator", foreground: c.decorator },
    { token: "operator", foreground: c.operator },
    { token: "delimiter", foreground: c.delimiter },
    // Markdown, JSON and the other languages Monaco colors itself.
    { token: "string.key.json", foreground: c.call },
    { token: "string.value.json", foreground: c.string },
    { token: "keyword.json", foreground: c.value },
  ];
}

// Graphite, violet and gold; the light one is the documentation's.
monaco.editor.defineTheme("gaanim", {
  base: "vs-dark",
  inherit: true,
  rules: syntaxRules(SYNTAX.dark),
  colors: {
    "editor.background": "#121212",
    "editor.foreground": "#e4e1f2",
    "editor.lineHighlightBackground": "#1b1b1b",
    "editorLineNumber.foreground": "#4a4a4a",
    "editorLineNumber.activeForeground": "#a0a0a0",
    "editorGutter.background": "#121212",
    "editorWidget.background": "#1b1b1b",
    "editorWidget.border": "#3d3d3d",
    "editorSuggestWidget.background": "#1b1b1b",
    "editorSuggestWidget.border": "#3d3d3d",
    "editorSuggestWidget.selectedBackground": "#2a2650",
    "editorSuggestWidget.highlightForeground": "#a99fff",
    "editorHoverWidget.background": "#1b1b1b",
    "editorHoverWidget.border": "#3d3d3d",
    "editorCursor.foreground": "#ffc933",
    "editor.selectionBackground": "#3f37a8aa",
    "editor.inactiveSelectionBackground": "#3f37a855",
    "editorBracketMatch.border": "#7c6cff",
    "editorBracketHighlight.foreground1": "#ffc933",
    "editorBracketHighlight.foreground2": "#a99fff",
    "editorBracketHighlight.foreground3": "#7cc7ff",
    "editorIndentGuide.background1": "#262626",
    "editorIndentGuide.activeBackground1": "#3d3d3d",
    "menu.background": "#1b1b1b",
    "menu.foreground": "#ededed",
    "menu.selectionBackground": "#2a2650",
    "menu.selectionForeground": "#c9c3ff",
    "menu.separatorBackground": "#3d3d3d",
    "menu.border": "#3d3d3d",
    // Lists (suggestions, the command palette): the selected row keeps
    // legible text on the violet.
    "editorSuggestWidget.foreground": "#e4e1f2",
    "editorSuggestWidget.selectedForeground": "#ffffff",
    "editorSuggestWidget.selectedIconForeground": "#ffc933",
    "editorSuggestWidget.focusHighlightForeground": "#c9c3ff",
    "list.activeSelectionBackground": "#2a2650",
    "list.activeSelectionForeground": "#ffffff",
    "list.inactiveSelectionBackground": "#242424",
    "list.hoverBackground": "#222222",
    "list.highlightForeground": "#a99fff",
    "quickInput.background": "#1b1b1b",
    "quickInputList.focusBackground": "#2a2650",
    "quickInputList.focusForeground": "#ffffff",
    "focusBorder": "#a99fff",
  },
});
monaco.editor.defineTheme("gaanim-light", {
  base: "vs",
  inherit: true,
  rules: syntaxRules(SYNTAX.light),
  colors: {
    "editor.background": "#ffffff",
    "editor.foreground": "#1f1d2b",
    "editor.lineHighlightBackground": "#f6f5ff",
    "editorLineNumber.foreground": "#b4b2c2",
    "editorLineNumber.activeForeground": "#5b4af0",
    "editorGutter.background": "#ffffff",
    "editorWidget.background": "#ffffff",
    "editorWidget.border": "#e5e5e7",
    "editorSuggestWidget.background": "#ffffff",
    "editorSuggestWidget.border": "#e5e5e7",
    "editorSuggestWidget.selectedBackground": "#e9e6ff",
    "editorSuggestWidget.highlightForeground": "#5b4af0",
    "editorHoverWidget.background": "#ffffff",
    "editorHoverWidget.border": "#e5e5e7",
    "editorCursor.foreground": "#5b4af0",
    "editor.selectionBackground": "#cbc6ff99",
    "editor.inactiveSelectionBackground": "#cbc6ff55",
    "editorBracketMatch.border": "#7c6cff",
    "editorBracketHighlight.foreground1": "#b45309",
    "editorBracketHighlight.foreground2": "#5b4af0",
    "editorBracketHighlight.foreground3": "#0369a1",
    "editorIndentGuide.background1": "#eeeef0",
    "editorIndentGuide.activeBackground1": "#d4d4d8",
    "menu.background": "#ffffff",
    "menu.foreground": "#18181b",
    "menu.selectionBackground": "#f1efff",
    "menu.selectionForeground": "#4636d6",
    "menu.separatorBackground": "#e5e5e7",
    "menu.border": "#d4d4d8",
    "editorSuggestWidget.foreground": "#1f1d2b",
    "editorSuggestWidget.selectedForeground": "#1f1d2b",
    "editorSuggestWidget.selectedIconForeground": "#5b4af0",
    "editorSuggestWidget.focusHighlightForeground": "#4636d6",
    "list.activeSelectionBackground": "#e9e6ff",
    "list.activeSelectionForeground": "#1f1d2b",
    "list.inactiveSelectionBackground": "#f0f0f2",
    "list.hoverBackground": "#f6f5ff",
    "list.highlightForeground": "#5b4af0",
    "quickInput.background": "#ffffff",
    "quickInputList.focusBackground": "#e9e6ff",
    "quickInputList.focusForeground": "#1f1d2b",
    "focusBorder": "#5b4af0",
  },
});

// The theme: the documentation site's choice (localStorage "theme", the
// same origin) or the system's.
const darkScheme = matchMedia("(prefers-color-scheme: dark)");
function storedTheme() {
  const theme = load("theme");
  return theme === "light" || theme === "dark" ? theme : null;
}
function currentTheme() {
  return storedTheme() ?? (darkScheme.matches ? "dark" : "light");
}
function editorTheme() {
  return currentTheme() === "dark" ? "gaanim" : "gaanim-light";
}

ui.editor.textContent = "";
const editor = monaco.editor.create(ui.editor, {
  model: null,
  theme: editorTheme(),
  automaticLayout: true,
  fontSize: 14,
  fontFamily: getComputedStyle(document.documentElement).getPropertyValue("--mono"),
  fontLigatures: true,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  tabSize: 4,
  insertSpaces: true,
  padding: { top: 10 },
  smoothScrolling: true,
  stickyScroll: { enabled: true },
  bracketPairColorization: { enabled: true },
  guides: { bracketPairs: "active", indentation: true },
  wordBasedSuggestions: "off",
  quickSuggestions: { other: true, comments: false, strings: true },
  suggest: { preview: true, showWords: false, insertMode: "replace" },
  suggestSelection: "first",
  fixedOverflowWidgets: true,
  glyphMargin: true,
});
// The code face loads after the editor measured its fallback.
document.fonts?.ready.then(() => monaco.editor.remeasureFonts());

function applyTheme() {
  const theme = storedTheme();
  if (theme) document.documentElement.dataset.theme = theme;
  else delete document.documentElement.dataset.theme;
  monaco.editor.setTheme(editorTheme());
  const next = currentTheme() === "dark" ? "claro" : "oscuro";
  ui.theme.title = `Usar el tema ${next}`;
  ui.theme.setAttribute("aria-label", ui.theme.title);
}
ui.theme.addEventListener("click", () => {
  store("theme", currentTheme() === "dark" ? "light" : "dark");
  applyTheme();
});
darkScheme.addEventListener("change", applyTheme);
// The documentation, open in another tab, changed it.
window.addEventListener("storage", (event) => {
  if (event.key === "theme") applyTheme();
});
applyTheme();

const LANGUAGES = { py: "python", pyi: "python", json: "json", md: "markdown", yaml: "yaml", yml: "yaml", css: "css", html: "html", xml: "xml", js: "javascript" };

// One model per text file, made when the file first opens.
const models = new Map();
// Where each model was scrolled, and its runtime error line.
const viewStates = new Map();
const errorMarks = new Map();

function uriOf(path) {
  return monaco.Uri.from({ scheme: "file", path: `/proyecto/${path}` });
}

function pathOf(model) {
  return model.uri.path.replace(/^\/proyecto\//, "");
}

function modelFor(path) {
  let model = models.get(path);
  if (!model) {
    model = monaco.editor.createModel(project.get(path).text, LANGUAGES[extension(path)] ?? "plaintext", uriOf(path));
    model.onDidChangeContent(() => edited(path, model));
    models.set(path, model);
  }
  return model;
}

function dropModel(path) {
  models.get(path)?.dispose();
  models.delete(path);
  viewStates.delete(path);
  errorMarks.delete(path);
}

function edited(path, model) {
  const file = project.get(path);
  if (!file) return;
  file.text = model.getValue();
  persist(path, { soon: true });
  // Edited code is no longer the shared one.
  if (/[#&](c|p)=/.test(location.hash)) history.replaceState(null, "", location.pathname + location.search);
  clearRuntimeError(path);
  if (model === editor.getModel()) scheduleSyntaxCheck();
  scheduleLanguageSync();
  scheduleLiveRun();
}

function clearRuntimeError(path) {
  const model = models.get(path);
  if (!model) return;
  monaco.editor.setModelMarkers(model, "runtime", []);
  errorMarks.set(path, model.deltaDecorations(errorMarks.get(path) ?? [], []));
}

function revealLine(line) {
  const model = editor.getModel();
  if (!model) return;
  const target = Math.min(line, model.getLineCount());
  editor.revealLineInCenterIfOutsideViewport(target);
  editor.setPosition({ lineNumber: target, column: model.getLineFirstNonWhitespaceColumn(target) || 1 });
  editor.focus();
}

editor.onDidChangeCursorPosition(({ position }) => {
  ui.cursor.textContent = `Ln ${position.lineNumber}, Col ${position.column}`;
});

editor.addAction({
  id: "gaanim.run",
  label: "Ejecutar escena",
  keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, monaco.KeyMod.Shift | monaco.KeyCode.Enter],
  contextMenuGroupId: "navigation",
  run: () => run(),
});
// Saving runs the scene, as a file watcher would.
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveAndRun());

function saveAndRun() {
  flushSaves();
  run();
}

// ---------------------------------------------------------------------------
// Editor tabs and asset previews
// ---------------------------------------------------------------------------

const tabsState = loadJson(projectKey(KEYS.tabs), {});
let openTabs = (Array.isArray(tabsState.open) ? tabsState.open : []).filter((path) => project.has(path));
if (!openTabs.includes(ENTRY)) openTabs.unshift(ENTRY);
let activePath = null;
let assetUrl = null;

function saveTabs() {
  store(projectKey(KEYS.tabs), JSON.stringify({ open: openTabs, active: activePath }));
}

// Open `path` in a tab: a text file in the editor, an asset in its preview.
function openFile(path, { line = null, focus = true } = {}) {
  if (!project.has(path)) return;
  if (!openTabs.includes(path)) openTabs.push(path);
  const current = editor.getModel();
  if (current) viewStates.set(pathOf(current), editor.saveViewState());
  activePath = path;
  const file = project.get(path);
  if (file.text !== undefined) {
    ui.assetView.hidden = true;
    ui.editor.hidden = false;
    editor.setModel(modelFor(path));
    const state = viewStates.get(path);
    if (state) editor.restoreViewState(state);
    if (line) revealLine(line);
    else if (focus) editor.focus();
    scheduleSyntaxCheck();
  } else {
    editor.setModel(null);
    ui.editor.hidden = true;
    showAsset(file);
  }
  renderTabs();
  renderTree();
  saveTabs();
}

function closeTab(path) {
  const index = openTabs.indexOf(path);
  if (index < 0) return;
  openTabs.splice(index, 1);
  if (activePath === path) {
    activePath = null;
    openFile(openTabs[Math.min(index, openTabs.length - 1)] ?? ENTRY);
  } else {
    renderTabs();
    saveTabs();
  }
}

function renderTabs() {
  const tabs = openTabs.map((path) => {
    const tab = document.createElement("div");
    tab.className = `editor-tab${path === activePath ? " active" : ""}`;
    tab.title = path;
    tab.setAttribute("role", "tab");
    tab.setAttribute("aria-selected", String(path === activePath));
    const icon = fileIcon(iconOf(path));
    const name = document.createElement("button");
    name.type = "button";
    name.className = "tab-name";
    name.textContent = baseName(path);
    name.addEventListener("click", () => openFile(path));
    tab.append(icon, name);
    if (path === ENTRY) {
      const entry = document.createElement("span");
      entry.className = "entry-mark";
      entry.title = "Se ejecuta";
      entry.textContent = "▶";
      tab.append(entry);
    } else {
      const close = document.createElement("button");
      close.type = "button";
      close.className = "tab-close";
      close.title = "Cerrar";
      close.setAttribute("aria-label", `Cerrar ${baseName(path)}`);
      close.textContent = "×";
      close.addEventListener("click", () => closeTab(path));
      tab.append(close);
    }
    tab.addEventListener("auxclick", (event) => {
      if (event.button === 1 && path !== ENTRY) closeTab(path);
    });
    return tab;
  });
  ui.editorTabs.replaceChildren(...tabs);
  ui.editorTabs.querySelector(".active")?.scrollIntoView({ block: "nearest", inline: "nearest" });
}

// The code that uses an asset from main.py, which sits at the project's root.
function snippetFor(path) {
  const quoted = JSON.stringify(path);
  switch (kindOf(path)) {
    case "image":
      return `scene.media.image(${quoted})`;
    case "svg":
      return `scene.media.svg(${quoted})`;
    case "audio":
      return `scene.media.audio(${quoted})`;
    case "video":
      return `scene.media.video(${quoted})`;
    case "font": {
      const family = project.get(path)?.family ?? baseName(path).replace(/\.[^.]+$/, "");
      return `Theme(font_dir=${JSON.stringify(parentOf(path) || ".")}, fonts={"text": ${JSON.stringify(family)}})`;
    }
    case "python":
      return `import ${path.replace(/\.pyi?$/, "").replace(/\//g, ".")}`;
    default:
      return quoted;
  }
}

function insertSnippet(path) {
  if (project.get(activePath)?.text === undefined) openFile(ENTRY);
  const text = snippetFor(path);
  editor.executeEdits("insert-file", [{ range: editor.getSelection(), text, forceMoveMarkers: true }]);
  editor.focus();
}

// The family a font file declares, which `Theme(fonts=…)` names: the name
// table's typographic family (16), or else its family (1).
function fontFamily(buffer) {
  try {
    const view = new DataView(buffer);
    let base = 0;
    if (view.getUint32(0) === 0x74746366) base = view.getUint32(12); // "ttcf": the first font
    const tables = view.getUint16(base + 4);
    for (let i = 0; i < tables; i++) {
      const record = base + 12 + i * 16;
      if (view.getUint32(record) !== 0x6e616d65) continue; // "name"
      const table = view.getUint32(record + 8);
      const count = view.getUint16(table + 2);
      const strings = table + view.getUint16(table + 4);
      const found = {};
      for (let j = 0; j < count; j++) {
        const entry = table + 6 + j * 12;
        const [platform, , , id, length, offset] = [0, 2, 4, 6, 8, 10].map((at) => view.getUint16(entry + at));
        if (id !== 1 && id !== 16) continue;
        const bytes = new Uint8Array(buffer, strings + offset, length);
        let name = "";
        if (platform === 3 || platform === 0) {
          for (let k = 0; k + 1 < bytes.length; k += 2) name += String.fromCharCode((bytes[k] << 8) | bytes[k + 1]);
        } else {
          name = String.fromCharCode(...bytes);
        }
        if (name && !found[id]) found[id] = name;
      }
      return found[16] ?? found[1] ?? null;
    }
  } catch {
    // Not a font this reads (WOFF is compressed): no family.
  }
  return null;
}

let previewFonts = 0;
function showAsset(file) {
  if (assetUrl) URL.revokeObjectURL(assetUrl);
  assetUrl = null;
  const kind = kindOf(file.path);
  const view = ui.assetView;
  view.replaceChildren();
  view.hidden = false;
  const stage = document.createElement("div");
  stage.className = "asset-stage";
  const type = file.type || (kind === "svg" ? "image/svg+xml" : "");
  if (kind === "image" || kind === "svg") {
    assetUrl = URL.createObjectURL(new Blob([file.bytes], { type }));
    const image = Object.assign(document.createElement("img"), { src: assetUrl, alt: file.path });
    image.addEventListener("load", () => {
      meta.textContent += ` · ${image.naturalWidth} × ${image.naturalHeight}`;
    });
    stage.append(image);
  } else if (kind === "audio" || kind === "video") {
    assetUrl = URL.createObjectURL(new Blob([file.bytes], { type }));
    const media = Object.assign(document.createElement(kind), { src: assetUrl, controls: true });
    stage.append(media);
  } else if (kind === "font") {
    const alias = `gaanim-preview-${++previewFonts}`;
    const sample = document.createElement("div");
    sample.className = "font-sample";
    sample.style.fontFamily = `"${alias}", var(--mono)`;
    sample.innerHTML = "<p>Aa Bb Cc Ññ 0123</p><p>El veloz murciélago hindú comía feliz cardillo y kiwi.</p>";
    new FontFace(alias, file.bytes).load().then((face) => document.fonts.add(face), () => {});
    stage.append(sample);
  } else {
    stage.append(fileIcon(iconOf(file.path), { big: true }));
  }
  const meta = document.createElement("p");
  meta.className = "asset-meta";
  meta.textContent = `${file.path} · ${formatSize(file.size ?? file.bytes.byteLength)}`;
  if (kind === "font" && file.family) meta.textContent += ` · familia «${file.family}»`;
  const code = document.createElement("code");
  code.className = "asset-snippet";
  code.textContent = snippetFor(file.path);
  const actions = document.createElement("div");
  actions.className = "asset-actions";
  const button = (label, handler, className = "ghost") => {
    const element = Object.assign(document.createElement("button"), { type: "button", className, textContent: label });
    element.addEventListener("click", handler);
    return element;
  };
  actions.append(
    button("Insertar en el código", () => insertSnippet(file.path), "primary"),
    button("Descargar", () => downloadBlob(baseName(file.path), file.bytes, type || "application/octet-stream")),
    button("Renombrar", () => startRename(file.path)),
    button("Eliminar", () => removePaths([file.path]), "ghost danger"),
  );
  view.append(stage, meta, code, actions);
}

// ---------------------------------------------------------------------------
// Explorer
// ---------------------------------------------------------------------------

const expanded = new Set(loadJson(projectKey(KEYS.expanded), []));
let selectedPath = null;
// The row being renamed or created: { path, folder, kind } while it edits.
let editing = null;

function saveExpanded() {
  store(projectKey(KEYS.expanded), JSON.stringify([...expanded]));
}

// The project as nested folders: { folders: Map(name → node), files: [path] }.
function treeOf() {
  const root = { path: "", folders: new Map(), files: [] };
  const nodeFor = (folder) => {
    if (!folder) return root;
    const parent = nodeFor(parentOf(folder));
    const name = baseName(folder);
    if (!parent.folders.has(name)) parent.folders.set(name, { path: folder, folders: new Map(), files: [] });
    return parent.folders.get(name);
  };
  for (const folder of allFolders()) nodeFor(folder);
  for (const path of project.keys()) nodeFor(parentOf(path)).files.push(path);
  return root;
}

const byName = (a, b) => a.localeCompare(b, "es", { sensitivity: "base", numeric: true });

function renderTree() {
  const rows = [];
  const add = (node, depth) => {
    const subfolders = [...node.folders.values()].sort((a, b) => byName(baseName(a.path), baseName(b.path)));
    for (const folder of subfolders) {
      const open = expanded.has(folder.path);
      rows.push(row({ path: folder.path, folder: true, open, depth }));
      if (editing?.creating && editing.parent === folder.path && open) rows.push(editRow(depth + 1));
      if (open) add(folder, depth + 1);
    }
    // main.py first: it is what runs.
    const files = node.files.sort((a, b) => (b === ENTRY) - (a === ENTRY) || byName(baseName(a), baseName(b)));
    for (const path of files) rows.push(row({ path, folder: false, depth }));
  };
  if (editing?.creating && editing.parent === "") rows.push(editRow(0));
  add(treeOf(), 0);
  ui.tree.replaceChildren(...rows);
  ui.tree.querySelector("input")?.focus();
}

function row({ path, folder, open = false, depth }) {
  const item = document.createElement("li");
  item.className = `row${folder ? " folder" : ""}${path === activePath ? " active" : ""}${path === selectedPath ? " selected" : ""}`;
  item.style.setProperty("--depth", depth);
  item.dataset.path = path;
  item.dataset.folder = folder ? "1" : "";
  item.setAttribute("role", "treeitem");
  item.tabIndex = path === selectedPath ? 0 : -1;
  item.draggable = path !== ENTRY;
  if (folder) item.setAttribute("aria-expanded", String(open));
  const chevron = document.createElement("span");
  chevron.className = "chevron";
  chevron.textContent = folder ? (open ? "▾" : "▸") : "";
  item.append(chevron, fileIcon(folder ? (open ? "folder-open" : "folder") : iconOf(path)));
  if (editing && !editing.creating && editing.path === path) {
    item.append(renameInput(baseName(path)));
  } else {
    const name = document.createElement("span");
    name.className = "name";
    name.textContent = baseName(path);
    item.append(name);
    if (path === ENTRY) {
      const entry = document.createElement("span");
      entry.className = "entry-mark";
      entry.title = "Se ejecuta al pulsar Ejecutar";
      entry.textContent = "▶";
      item.append(entry);
    }
  }
  return item;
}

// A row with a name to type, for a new file or folder.
function editRow(depth) {
  const item = document.createElement("li");
  item.className = "row editing";
  item.style.setProperty("--depth", depth);
  const chevron = document.createElement("span");
  chevron.className = "chevron";
  item.append(chevron, fileIcon(editing.kind === "folder" ? "folder" : "file"), renameInput(""));
  return item;
}

function renameInput(value) {
  const input = Object.assign(document.createElement("input"), { className: "rename", value, spellcheck: false });
  input.setAttribute("aria-label", "Nombre");
  // The name without its extension is selected, as editors do.
  requestAnimationFrame(() => {
    const dot = value.lastIndexOf(".");
    input.setSelectionRange(0, dot > 0 ? dot : value.length);
  });
  let done = false;
  const finish = (accept) => {
    if (done) return;
    done = true;
    const name = input.value.trim();
    const pending = editing;
    editing = null;
    if (accept && name) {
      if (pending.creating) create(pending.parent, name, pending.kind);
      else rename(pending.path, name);
    }
    renderTree();
  };
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    else if (event.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
  input.addEventListener("click", (event) => event.stopPropagation());
  return input;
}

// The folder new files go to: the selected folder, or the selected file's.
function targetFolder() {
  if (!selectedPath) return "";
  return allFolders().has(selectedPath) ? selectedPath : parentOf(selectedPath);
}

function startCreate(kind, parent = targetFolder()) {
  if (parent) expanded.add(parent);
  editing = { creating: true, parent, kind };
  showExplorer();
  renderTree();
}

function startRename(path) {
  if (path === ENTRY) {
    toast("main.py es el archivo que se ejecuta: no se renombra.");
    return;
  }
  editing = { path };
  showExplorer();
  renderTree();
}

function create(parent, name, kind) {
  const path = cleanPath(join(parent, name));
  if (!path) return;
  if (exists(path)) {
    toast(`Ya existe «${path}».`);
    return;
  }
  reveal(path);
  if (kind === "folder") {
    folders.add(path);
    expanded.add(path);
    saveExpanded();
    persist(path);
    selectedPath = path;
    return;
  }
  setFile(isText(path) ? { path, text: "" } : { path, bytes: new ArrayBuffer(0), type: "", size: 0, modified: Date.now() });
  selectedPath = path;
  openFile(path);
}

function setFile(file) {
  if (file.bytes && kindOf(file.path) === "font") file.family = fontFamily(file.bytes);
  const previous = project.get(file.path);
  project.set(file.path, file);
  if (previous?.text !== undefined && file.text !== undefined && models.has(file.path)) {
    models.get(file.path).setValue(file.text);
  } else if (models.has(file.path)) {
    dropModel(file.path);
  }
  persist(file.path);
  scheduleLanguageSync();
}

function rename(path, name) {
  const target = cleanPath(join(parentOf(path), name));
  if (!target || target === path) return;
  move(path, target);
}

// Move a file or a folder (with what it holds) to `target`.
function move(path, target) {
  if (exists(target)) {
    toast(`Ya existe «${target}».`);
    return;
  }
  if (target === path || target.startsWith(`${path}/`)) return;
  const prefix = `${path}/`;
  const moves = [...project.keys()].filter((key) => key === path || key.startsWith(prefix));
  if (moves.includes(ENTRY)) {
    toast("main.py se queda en la raíz: es el archivo que se ejecuta.");
    return;
  }
  const renamed = (key) => target + key.slice(path.length);
  const wasActive = activePath;
  for (const key of moves) {
    const file = project.get(key);
    project.delete(key);
    persist(key);
    const moved = { ...file, path: renamed(key) };
    if (file.text !== undefined && models.has(key)) moved.text = models.get(key).getValue();
    project.set(moved.path, moved);
    persist(moved.path);
    dropModel(key);
    const tab = openTabs.indexOf(key);
    if (tab >= 0) openTabs[tab] = moved.path;
  }
  for (const folder of [...folders]) {
    if (folder === path || folder.startsWith(prefix)) {
      folders.delete(folder);
      persist(folder);
      folders.add(renamed(folder));
      persist(renamed(folder));
    }
  }
  for (const folder of [...expanded]) {
    if (folder === path || folder.startsWith(prefix)) {
      expanded.delete(folder);
      expanded.add(renamed(folder));
    }
  }
  reveal(target);
  if (moves.some((key) => project.get(renamed(key))?.text === undefined)) assetsDirty = true;
  selectedPath = target;
  activePath = null;
  openFile(moves.includes(wasActive) ? renamed(wasActive) : wasActive ?? ENTRY, { focus: false });
  scheduleLanguageSync();
}

// Delete files and folders, with a moment to take it back.
function removePaths(paths) {
  const removed = [];
  const removedFolders = [];
  for (const path of paths) {
    if (path === ENTRY) {
      toast("main.py es el archivo que se ejecuta: no se elimina.");
      return;
    }
    const prefix = `${path}/`;
    for (const key of [...project.keys()]) {
      if (key === path || key.startsWith(prefix)) {
        if (key === ENTRY) {
          toast("Esa carpeta no puede irse: tiene main.py.");
          return;
        }
        removed.push(project.get(key));
      }
    }
    for (const folder of allFolders()) {
      if (folder === path || folder.startsWith(prefix)) removedFolders.push(folder);
    }
  }
  for (const file of removed) {
    if (models.has(file.path)) file.text = models.get(file.path).getValue();
    project.delete(file.path);
    persist(file.path);
    dropModel(file.path);
    if (file.text === undefined) assetsDirty = true;
  }
  for (const folder of removedFolders) {
    folders.delete(folder);
    persist(folder);
  }
  openTabs = openTabs.filter((path) => project.has(path));
  if (!project.has(activePath)) activePath = null;
  selectedPath = null;
  openFile(activePath ?? openTabs.at(-1) ?? ENTRY, { focus: false });
  scheduleLanguageSync();
  const label = paths.length === 1 ? `«${baseName(paths[0])}»` : `${paths.length} elementos`;
  toast(`Se eliminó ${label}.`, 6000, {
    label: "Deshacer",
    run() {
      for (const folder of removedFolders) {
        folders.add(folder);
        persist(folder);
      }
      for (const file of removed) setFile(file);
      renderTree();
    },
  });
}

function toggleFolder(path) {
  if (expanded.has(path)) expanded.delete(path);
  else expanded.add(path);
  saveExpanded();
  renderTree();
}

function select(path) {
  selectedPath = path;
  for (const item of ui.tree.querySelectorAll(".row")) {
    const selected = item.dataset.path === path;
    item.classList.toggle("selected", selected);
    item.tabIndex = selected ? 0 : -1;
    if (selected) item.focus({ preventScroll: false });
  }
}

ui.tree.addEventListener("click", (event) => {
  const item = event.target.closest(".row[data-path]");
  if (!item) {
    select(null);
    return;
  }
  const path = item.dataset.path;
  select(path);
  if (item.dataset.folder) toggleFolder(path);
  else openFile(path, { focus: false });
});
ui.tree.addEventListener("dblclick", (event) => {
  const item = event.target.closest(".row[data-path]");
  if (item && !item.dataset.folder) openFile(item.dataset.path);
});
ui.tree.addEventListener("keydown", (event) => {
  if (editing) return;
  const items = [...ui.tree.querySelectorAll(".row[data-path]")];
  const index = items.findIndex((item) => item.dataset.path === selectedPath);
  const current = items[index];
  const path = current?.dataset.path;
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    const next = items[Math.max(0, Math.min(items.length - 1, index + (event.key === "ArrowDown" ? 1 : -1)))];
    if (next) select(next.dataset.path);
  } else if (!current) {
    return;
  } else if (event.key === "Enter") {
    event.preventDefault();
    if (current.dataset.folder) toggleFolder(path);
    else openFile(path);
  } else if (event.key === "ArrowRight" && current.dataset.folder && !expanded.has(path)) {
    toggleFolder(path);
  } else if (event.key === "ArrowLeft" && current.dataset.folder && expanded.has(path)) {
    toggleFolder(path);
  } else if (event.key === "F2") {
    event.preventDefault();
    startRename(path);
  } else if (event.key === "Delete") {
    event.preventDefault();
    removePaths([path]);
  }
});

// Context menu of a row, or of the tree's empty space.
function showContextMenu(x, y, items) {
  const menu = ui.contextMenu;
  menu.replaceChildren(
    ...items.map(({ label, run: action, danger, current }) => {
      if (!label) return Object.assign(document.createElement("hr"), {});
      const button = Object.assign(document.createElement("button"), { type: "button", textContent: label });
      button.setAttribute("role", "menuitem");
      if (danger) button.className = "danger";
      if (current) {
        button.className = "current";
        button.setAttribute("aria-current", "true");
      }
      button.addEventListener("click", () => {
        hideContextMenu();
        action();
      });
      return button;
    }),
  );
  menu.hidden = false;
  const { width, height } = menu.getBoundingClientRect();
  menu.style.left = `${Math.min(x, innerWidth - width - 8)}px`;
  menu.style.top = `${Math.min(y, innerHeight - height - 8)}px`;
  menu.querySelector("button")?.focus();
}

function hideContextMenu() {
  ui.contextMenu.hidden = true;
}

ui.tree.addEventListener("contextmenu", (event) => {
  event.preventDefault();
  const item = event.target.closest(".row[data-path]");
  const path = item?.dataset.path ?? null;
  select(path);
  const folder = item ? (item.dataset.folder ? path : parentOf(path)) : "";
  const items = [];
  if (item && !item.dataset.folder) {
    items.push({ label: "Abrir", run: () => openFile(path) });
    if (path !== ENTRY) items.push({ label: "Insertar en el código", run: () => insertSnippet(path) });
    items.push({ label: "Descargar", run: () => downloadFile(path) });
    items.push({});
  }
  items.push({ label: "Nuevo archivo…", run: () => startCreate("file", folder) });
  items.push({ label: "Nueva carpeta…", run: () => startCreate("folder", folder) });
  items.push({ label: "Subir archivos…", run: () => pickFiles(folder, false) });
  items.push({ label: "Subir una carpeta…", run: () => pickFiles(folder, true) });
  if (item && path !== ENTRY) {
    items.push({});
    items.push({ label: "Renombrar", run: () => startRename(path) });
    items.push({ label: "Eliminar", run: () => removePaths([path]), danger: true });
  }
  showContextMenu(event.clientX, event.clientY, items);
});
document.addEventListener("pointerdown", (event) => {
  if (!ui.contextMenu.contains(event.target)) hideContextMenu();
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape") hideContextMenu();
});

function downloadFile(path) {
  const file = project.get(path);
  if (!file) return;
  if (file.text !== undefined) downloadBlob(baseName(path), models.get(path)?.getValue() ?? file.text, "text/plain");
  else downloadBlob(baseName(path), file.bytes, file.type || "application/octet-stream");
}

ui.newFile.addEventListener("click", () => startCreate("file"));
ui.newFolder.addEventListener("click", () => startCreate("folder"));

// ---------------------------------------------------------------------------
// Uploads: files and folders, by button or by dropping them
// ---------------------------------------------------------------------------

let uploadFolder = "";
function pickFiles(folder, wholeFolder) {
  uploadFolder = folder;
  (wholeFolder ? ui.folderInput : ui.fileInput).click();
}
ui.uploadFiles.addEventListener("click", () => pickFiles(targetFolder(), false));
ui.uploadFolder.addEventListener("click", () => pickFiles(targetFolder(), true));
for (const input of [ui.fileInput, ui.folderInput]) {
  input.addEventListener("change", () => {
    const entries = [...input.files].map((file) => ({ file, path: file.webkitRelativePath || file.name }));
    input.value = "";
    addEntries(entries, uploadFolder);
  });
}

// Hidden files and Python caches stay out of the project.
function ignored(path) {
  return path.split("/").some((part) => part.startsWith(".") || part === "__pycache__");
}

// Add `entries` ({ file, path } and { folder }) under `parent`.
async function addEntries(entries, parent = "") {
  let added = 0;
  let replaced = 0;
  for (const entry of entries) {
    if (entry.folder !== undefined) {
      const folder = cleanPath(join(parent, entry.folder));
      if (folder && !ignored(folder) && !allFolders().has(folder)) {
        folders.add(folder);
        persist(folder);
      }
      continue;
    }
    const path = cleanPath(join(parent, entry.path));
    if (!path || ignored(path)) continue;
    if (path === ENTRY && !isText(entry.file.name)) continue;
    if (entry.file.size > MAX_FILE) {
      toast(`«${path}» pesa ${formatSize(entry.file.size)}: el límite es ${formatSize(MAX_FILE)}.`);
      continue;
    }
    if (allFolders().has(path)) {
      toast(`Ya hay una carpeta «${path}».`);
      continue;
    }
    if (project.has(path)) replaced++;
    if (isText(path)) {
      setFile({ path, text: await entry.file.text() });
    } else {
      const bytes = await entry.file.arrayBuffer();
      setFile({ path, bytes, type: entry.file.type, size: entry.file.size, modified: Date.now() });
      assetsDirty = true;
    }
    reveal(path);
    added++;
  }
  renderTree();
  if (activePath && project.get(activePath)?.text === undefined) showAsset(project.get(activePath));
  if (added) {
    showExplorer();
    toast(
      added === 1
        ? replaced ? "Archivo reemplazado." : "Archivo añadido."
        : `${added} archivos añadidos${replaced ? `, ${replaced} reemplazados` : ""}.`,
    );
  }
}

// What was dropped, folders walked through: entries for `addEntries`.
async function droppedEntries(dataTransfer) {
  // The entries must be taken during the drop event, before any await.
  const roots = [...(dataTransfer.items ?? [])].map((item) => item.webkitGetAsEntry?.()).filter(Boolean);
  if (!roots.length) return [...dataTransfer.files].map((file) => ({ file, path: file.name }));
  const entries = [];
  const walk = async (entry, prefix) => {
    if (entry.isFile) {
      const file = await new Promise((resolve, reject) => entry.file(resolve, reject));
      entries.push({ file, path: prefix + entry.name });
    } else if (entry.isDirectory) {
      entries.push({ folder: prefix + entry.name });
      const reader = entry.createReader();
      for (;;) {
        const batch = await new Promise((resolve, reject) => reader.readEntries(resolve, reject));
        if (!batch.length) break;
        for (const child of batch) await walk(child, `${prefix}${entry.name}/`);
      }
    }
  };
  for (const root of roots) await walk(root, "");
  return entries;
}

const MOVE_TYPE = "application/x-gaanim-path";
const hasFiles = (event) => [...(event.dataTransfer?.types ?? [])].includes("Files");
const isMove = (event) => [...(event.dataTransfer?.types ?? [])].includes(MOVE_TYPE);

// Rows drag to move; folders (and the tree's empty space) take drops.
ui.tree.addEventListener("dragstart", (event) => {
  const item = event.target.closest(".row[data-path]");
  if (!item || item.dataset.path === ENTRY) return;
  event.dataTransfer.setData(MOVE_TYPE, item.dataset.path);
  event.dataTransfer.effectAllowed = "move";
});
function dropFolderOf(event) {
  const item = event.target.closest(".row[data-path]");
  if (!item) return "";
  return item.dataset.folder ? item.dataset.path : parentOf(item.dataset.path);
}
ui.tree.addEventListener("dragover", (event) => {
  if (!hasFiles(event) && !isMove(event)) return;
  event.preventDefault();
  event.stopPropagation();
  const folder = dropFolderOf(event);
  for (const item of ui.tree.querySelectorAll(".row")) {
    item.classList.toggle("drop-target", Boolean(folder) && item.dataset.path === folder);
  }
  ui.tree.classList.toggle("drop-root", !folder);
});
ui.tree.addEventListener("dragleave", (event) => {
  if (!ui.tree.contains(event.relatedTarget)) clearDropMarks();
});
function clearDropMarks() {
  ui.tree.classList.remove("drop-root");
  for (const item of ui.tree.querySelectorAll(".drop-target")) item.classList.remove("drop-target");
}
ui.tree.addEventListener("drop", async (event) => {
  if (!hasFiles(event) && !isMove(event)) return;
  event.preventDefault();
  event.stopPropagation();
  clearDropMarks();
  dragDepth = 0;
  ui.dropping.hidden = true;
  const folder = dropFolderOf(event);
  if (isMove(event)) {
    const path = event.dataTransfer.getData(MOVE_TYPE);
    const target = join(folder, baseName(path));
    if (path && target !== path) {
      if (folder) expanded.add(folder);
      move(path, target);
    }
    return;
  }
  addEntries(await droppedEntries(event.dataTransfer), folder);
});

// Files dropped anywhere else on the page go to the project's root.
let dragDepth = 0;
window.addEventListener("dragenter", (event) => {
  if (!hasFiles(event)) return;
  dragDepth++;
  ui.dropping.hidden = false;
});
window.addEventListener("dragleave", (event) => {
  if (!hasFiles(event)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) ui.dropping.hidden = true;
});
window.addEventListener("dragover", (event) => {
  if (hasFiles(event)) event.preventDefault();
});
window.addEventListener("drop", async (event) => {
  if (!hasFiles(event)) return;
  event.preventDefault();
  dragDepth = 0;
  ui.dropping.hidden = true;
  addEntries(await droppedEntries(event.dataTransfer), "");
});

// The explorer folds away; on narrow screens it opens over the editor.
const narrow = matchMedia("(max-width: 860px)");
function showExplorer() {
  document.body.classList.remove("explorer-hidden");
  if (narrow.matches) document.body.classList.add("explorer-open");
}
ui.explorerToggle.addEventListener("click", () => {
  if (narrow.matches) {
    document.body.classList.toggle("explorer-open");
  } else {
    document.body.classList.toggle("explorer-hidden");
    store(KEYS.explorer, document.body.classList.contains("explorer-hidden") ? "hidden" : "shown");
  }
});
if (load(KEYS.explorer) === "hidden") document.body.classList.add("explorer-hidden");
ui.tree.addEventListener("click", (event) => {
  if (narrow.matches && event.target.closest(".row[data-path]:not(.folder)")) document.body.classList.remove("explorer-open");
});
// A press outside the drawer closes it.
document.addEventListener("pointerdown", (event) => {
  if (!document.body.classList.contains("explorer-open")) return;
  if (ui.explorer.contains(event.target) || ui.explorerToggle.contains(event.target) || ui.contextMenu.contains(event.target)) return;
  document.body.classList.remove("explorer-open");
});

// ---------------------------------------------------------------------------
// Download: the project as a ZIP archive
// ---------------------------------------------------------------------------

const CRC_TABLE = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});

function crc32(bytes) {
  let crc = 0xffffffff;
  for (const byte of bytes) crc = CRC_TABLE[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

// A ZIP archive of `entries` ({ path, bytes }), stored without compression.
function zipArchive(entries) {
  const encoder = new TextEncoder();
  const parts = [];
  const central = [];
  let offset = 0;
  for (const { path, bytes } of entries) {
    const name = encoder.encode(path);
    const crc = crc32(bytes);
    const local = new DataView(new ArrayBuffer(30));
    local.setUint32(0, 0x04034b50, true);
    local.setUint16(4, 20, true);
    local.setUint16(6, 0x0800, true); // UTF-8 names
    local.setUint32(14, crc, true);
    local.setUint32(18, bytes.length, true);
    local.setUint32(22, bytes.length, true);
    local.setUint16(26, name.length, true);
    parts.push(new Uint8Array(local.buffer), name, bytes);
    const record = new DataView(new ArrayBuffer(46));
    record.setUint32(0, 0x02014b50, true);
    record.setUint16(4, 20, true);
    record.setUint16(6, 20, true);
    record.setUint16(8, 0x0800, true);
    record.setUint32(16, crc, true);
    record.setUint32(20, bytes.length, true);
    record.setUint32(24, bytes.length, true);
    record.setUint16(28, name.length, true);
    record.setUint32(42, offset, true);
    central.push(new Uint8Array(record.buffer), name);
    offset += 30 + name.length + bytes.length;
  }
  const size = central.reduce((sum, part) => sum + part.length, 0);
  const end = new DataView(new ArrayBuffer(22));
  end.setUint32(0, 0x06054b50, true);
  end.setUint16(8, entries.length, true);
  end.setUint16(10, entries.length, true);
  end.setUint32(12, size, true);
  end.setUint32(16, offset, true);
  return new Blob([...parts, ...central, new Uint8Array(end.buffer)], { type: "application/zip" });
}

// A name for files and folders: lowercase, without accents, spaces or signs.
function slug(text, fallback = "proyecto") {
  const name = text
    .trim()
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "");
  return name || fallback;
}

function downloadProject() {
  flushSaves();
  const encoder = new TextEncoder();
  const folder = slug(currentName());
  const entries = [...project.values()]
    .sort((a, b) => byName(a.path, b.path))
    .map((file) => ({
      path: `${folder}/${file.path}`,
      bytes: file.text !== undefined ? encoder.encode(models.get(file.path)?.getValue() ?? file.text) : new Uint8Array(file.bytes),
    }));
  downloadBlob(`${folder}.zip`, zipArchive(entries), "application/zip");
}

// The entries of a ZIP archive: { path, bytes } and { folder }. Stored and
// deflated entries are read (DecompressionStream inflates the latter).
async function readZip(buffer) {
  const view = new DataView(buffer);
  let end = buffer.byteLength - 22;
  while (end >= Math.max(0, buffer.byteLength - 65557) && view.getUint32(end, true) !== 0x06054b50) end--;
  if (end < 0 || view.getUint32(end, true) !== 0x06054b50) throw new Error("no es un archivo ZIP");
  const count = view.getUint16(end + 10, true);
  let at = view.getUint32(end + 16, true);
  const entries = [];
  for (let index = 0; index < count; index++) {
    if (view.getUint32(at, true) !== 0x02014b50) throw new Error("el directorio del ZIP está dañado");
    const flags = view.getUint16(at + 8, true);
    const method = view.getUint16(at + 10, true);
    const size = view.getUint32(at + 20, true);
    const nameLength = view.getUint16(at + 28, true);
    const extraLength = view.getUint16(at + 30, true);
    const commentLength = view.getUint16(at + 32, true);
    const local = view.getUint32(at + 42, true);
    const nameBytes = new Uint8Array(buffer, at + 46, nameLength);
    const name = flags & 0x0800 ? new TextDecoder().decode(nameBytes) : String.fromCharCode(...nameBytes);
    at += 46 + nameLength + extraLength + commentLength;
    if (name.endsWith("/")) {
      entries.push({ folder: name.slice(0, -1) });
      continue;
    }
    const start = local + 30 + view.getUint16(local + 26, true) + view.getUint16(local + 28, true);
    const data = new Uint8Array(buffer, start, size);
    if (method === 0) {
      entries.push({ path: name, bytes: data.slice() });
    } else if (method === 8) {
      const stream = new Blob([data]).stream().pipeThrough(new DecompressionStream("deflate-raw"));
      entries.push({ path: name, bytes: new Uint8Array(await new Response(stream).arrayBuffer()) });
    }
  }
  return entries;
}

// ---------------------------------------------------------------------------
// Projects
// ---------------------------------------------------------------------------

function showProjectName() {
  ui.projectName.textContent = currentName();
  ui.downloadProject.textContent = `${slug(currentName())}.zip`;
  document.title = `${currentName()} · Playground Gaanim`;
}

// Open project `id`; `files` and `folders`, when given, fill a new one.
async function switchProject(id, { files = null, newFolders = [] } = {}) {
  flushSaves();
  if (/[#&](c|p)=/.test(location.hash)) history.replaceState(null, "", location.pathname + location.search);
  for (const path of [...models.keys()]) dropModel(path);
  editor.setModel(null);
  currentProject = id;
  saveProjects();
  await loadProject(id);
  for (const file of files ?? []) {
    project.set(file.path, file);
    persist(file.path);
  }
  for (const folder of newFolders) {
    folders.add(folder);
    persist(folder);
  }
  ensureEntry();
  expanded.clear();
  for (const folder of loadJson(projectKey(KEYS.expanded), [])) expanded.add(folder);
  const state = loadJson(projectKey(KEYS.tabs), {});
  openTabs = (Array.isArray(state.open) ? state.open : []).filter((path) => project.has(path));
  if (!openTabs.includes(ENTRY)) openTabs.unshift(ENTRY);
  activePath = null;
  selectedPath = null;
  openFile(project.has(state.active) ? state.active : ENTRY, { focus: false });
  showProjectName();
  sentAssets = new Map();
  assetsDirty = true;
  scheduleLanguageSync();
  run();
}

// "Proyecto 2", "Proyecto 3"…: the first name no project has.
function freshName(base) {
  const names = new Set(projects.map((entry) => entry.name));
  if (!names.has(base)) return base;
  for (let n = 2; ; n++) if (!names.has(`${base} ${n}`)) return `${base} ${n}`;
}

function newProject() {
  const id = newProjectId();
  projects.push({ id, name: freshName("Proyecto") });
  switchProject(id, { files: [{ path: ENTRY, text: FALLBACK_CODE }] });
}

// An example of the gallery as a project of its own, named after it: the
// projects there are stay as they are.
function newFromExample(example) {
  const id = newProjectId();
  projects.push({ id, name: freshName(example.title) });
  switchProject(id, { files: [{ path: ENTRY, text: example.code }] });
}

function showExamplesMenu(x, y) {
  showContextMenu(x, y, [
    { label: "← Proyectos", run: () => showProjectMenu() },
    {},
    ...examples.map((example) => ({ label: example.title, run: () => newFromExample(example) })),
  ]);
}

function duplicateProject() {
  flushSaves();
  const files = [...project.values()].map((file) =>
    file.text !== undefined ? { path: file.path, text: models.get(file.path)?.getValue() ?? file.text } : { ...file },
  );
  const id = newProjectId();
  projects.push({ id, name: freshName(`${currentName()} (copia)`) });
  switchProject(id, { files, newFolders: [...folders] });
}

function startProjectRename() {
  const input = Object.assign(document.createElement("input"), { className: "rename project-rename", value: currentName(), spellcheck: false });
  input.setAttribute("aria-label", "Nombre del proyecto");
  ui.projectSwitch.hidden = true;
  ui.projectSwitch.after(input);
  input.focus();
  input.select();
  let done = false;
  const finish = (accept) => {
    if (done) return;
    done = true;
    const name = input.value.trim();
    if (accept && name) {
      projects.find((entry) => entry.id === currentProject).name = name;
      saveProjects();
      showProjectName();
    }
    input.remove();
    ui.projectSwitch.hidden = false;
  };
  input.addEventListener("keydown", (event) => {
    event.stopPropagation();
    if (event.key === "Enter") finish(true);
    else if (event.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
}

async function deleteProject() {
  flushSaves();
  const gone = projects.find((entry) => entry.id === currentProject);
  const records = (await transaction("readonly", (s) => s.getAll(projectRange(gone.id)))) ?? [];
  await transaction("readwrite", (s) => s.delete(projectRange(gone.id)));
  projects = projects.filter((entry) => entry !== gone);
  if (!projects.length) projects.push({ id: newProjectId(), name: "Mi proyecto" });
  await switchProject(projects[0].id);
  toast(`Se eliminó el proyecto «${gone.name}».`, 8000, {
    label: "Deshacer",
    async run() {
      projects.push(gone);
      await transaction("readwrite", (s) => {
        for (const record of records) s.put(record);
      });
      switchProject(gone.id);
    },
  });
}

// A ZIP archive (a project downloaded from here, or a folder of the
// application's) opens as a new project.
async function importZip(file) {
  let entries;
  try {
    entries = await readZip(await file.arrayBuffer());
  } catch (error) {
    toast(`No se pudo abrir «${file.name}»: ${error.message}.`);
    return;
  }
  // One folder holding everything, as a zipped folder: its content is the
  // project.
  const names = entries.map((entry) => entry.path ?? entry.folder);
  const top = names[0]?.split("/")[0];
  const nested = top && entries.some((entry) => entry.path?.startsWith(`${top}/`)) && names.every((name) => name === top || name.startsWith(`${top}/`));
  const files = [];
  const newFolders = [];
  for (const entry of entries) {
    const name = entry.path ?? entry.folder;
    const path = cleanPath(nested ? name.slice(top.length + 1) : name);
    if (!path || ignored(path)) continue;
    if (entry.folder !== undefined) {
      newFolders.push(path);
    } else if (isText(path)) {
      files.push({ path, text: new TextDecoder().decode(entry.bytes) });
    } else {
      const bytes = entry.bytes.buffer;
      const asset = { path, bytes, type: "", size: bytes.byteLength, modified: Date.now() };
      if (kindOf(path) === "font") asset.family = fontFamily(bytes);
      files.push(asset);
    }
  }
  if (!files.length) {
    toast(`«${file.name}» no tiene archivos.`);
    return;
  }
  const id = newProjectId();
  const name = freshName(file.name.replace(/\.zip$/i, "") || "Proyecto");
  projects.push({ id, name });
  const hadEntry = files.some((entry) => entry.path === ENTRY);
  await switchProject(id, { files, newFolders });
  toast(hadEntry ? `Se abrió «${name}».` : `Se abrió «${name}». No traía main.py: se creó uno, que es el que se ejecuta.`, 5000);
}

function showProjectMenu() {
  const rect = ui.projectSwitch.getBoundingClientRect();
  const items = projects.map((entry) => ({
    label: entry.name,
    current: entry.id === currentProject,
    run: () => entry.id !== currentProject && switchProject(entry.id),
  }));
  items.push(
    {},
    { label: "Nuevo proyecto", run: newProject },
    { label: "Nuevo desde un ejemplo…", run: () => showExamplesMenu(rect.left, rect.bottom + 4) },
    { label: "Abrir un .zip…", run: () => ui.zipInput.click() },
    { label: "Duplicar", run: duplicateProject },
    { label: "Renombrar…", run: startProjectRename },
    { label: "Descargar .zip", run: downloadProject },
    {},
    { label: "Eliminar este proyecto", run: deleteProject, danger: true },
  );
  showContextMenu(rect.left, rect.bottom + 4, items);
}

ui.projectSwitch.addEventListener("click", (event) => {
  event.stopPropagation();
  showProjectMenu();
});
ui.zipInput.addEventListener("change", () => {
  const file = ui.zipInput.files[0];
  ui.zipInput.value = "";
  if (file) importZip(file);
});
showProjectName();

// ---------------------------------------------------------------------------
// Language services (language.js)
// ---------------------------------------------------------------------------

// Jedi loads a Python of its own: it starts once the runner's is ready, the
// editor gets the focus, or a few seconds pass, so that the two do not
// download and compile at once and the first run comes sooner.
let language = null;
let requestId = 0;
const waiting = new Map();

function startLanguage() {
  if (language) return;
  // The version in the address: a release never runs the last one's cached worker.
  language = new Worker(`language.js?v=${config.version}`, { type: "module" });
  language.onmessage = onLanguageMessage;
  language.postMessage({ type: "init", config });
}
editor.onDidFocusEditorText(startLanguage);
setTimeout(startLanguage, 6000);

function onLanguageMessage({ data }) {
  if (data.type === "ready") {
    languageReady = true;
    syncLanguageProject();
    scheduleSyntaxCheck();
    return;
  }
  if (data.type === "failed") {
    log(`El autocompletado no pudo iniciarse: ${data.error}`, "stderr");
    return;
  }
  const resolve = waiting.get(data.id);
  waiting.delete(data.id);
  resolve?.(data.result ?? null);
}

function ask(method, ...args) {
  if (!languageReady) return Promise.resolve(null);
  const id = ++requestId;
  return new Promise((resolve) => {
    waiting.set(id, resolve);
    language.postMessage({ id, method, args });
  });
}

// Jedi sees the project as it is: modules with their text, assets as names.
function syncLanguageProject() {
  const files = {};
  for (const [path, file] of project) files[path] = file.text !== undefined ? (models.get(path)?.getValue() ?? file.text) : null;
  ask("set_project", files);
}

function scheduleLanguageSync() {
  clearTimeout(languageTimer);
  languageTimer = setTimeout(syncLanguageProject, 400);
}

const KINDS = {
  module: monaco.languages.CompletionItemKind.Module,
  class: monaco.languages.CompletionItemKind.Class,
  instance: monaco.languages.CompletionItemKind.Variable,
  function: monaco.languages.CompletionItemKind.Function,
  param: monaco.languages.CompletionItemKind.Variable,
  path: monaco.languages.CompletionItemKind.File,
  keyword: monaco.languages.CompletionItemKind.Keyword,
  property: monaco.languages.CompletionItemKind.Property,
  statement: monaco.languages.CompletionItemKind.Variable,
};

// Jedi gives raw docstrings: turn reST literals and indented examples into
// Markdown code.
function docToMarkdown(doc) {
  if (!doc) return "";
  const lines = doc.replace(/``([^`]+)``/g, "`$1`").split("\n");
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const indented = /^( {4}|\t)/.test(lines[i]);
    if (indented && (i === 0 || lines[i - 1].trim() === "" || lines[i - 1].trim().endsWith(":"))) {
      const block = [];
      while (i < lines.length && (/^( {4}|\t)/.test(lines[i]) || lines[i].trim() === "")) {
        block.push(lines[i].replace(/^( {4}|\t)/, ""));
        i++;
      }
      while (block.length && block.at(-1).trim() === "") block.pop();
      out.push("", "```python", ...block, "```", "");
      i--;
    } else {
      out.push(lines[i]);
    }
  }
  return out.join("\n");
}

// A long signature with one parameter per line, as a formatter writes it,
// so that it reads in a narrow panel.
function formatSignature(text) {
  const open = text.indexOf("(");
  if (text.length <= 72 || open < 0) return text;
  let depth = 0;
  let quote = null;
  let start = open + 1;
  let close = -1;
  const params = [];
  for (let at = start; at < text.length && close < 0; at++) {
    const char = text[at];
    if (quote) {
      if (char === quote) quote = null;
    } else if (char === '"' || char === "'") {
      quote = char;
    } else if ("([{".includes(char)) {
      depth++;
    } else if (")]}".includes(char)) {
      if (depth === 0) close = at;
      else depth--;
    } else if (char === "," && depth === 0) {
      params.push(text.slice(start, at).trim());
      start = at + 1;
    }
  }
  if (close < 0) return text;
  const last = text.slice(start, close).trim();
  if (last) params.push(last);
  if (!params.length) return text;
  return `${text.slice(0, open)}(\n${params.map((param) => `    ${param},`).join("\n")}\n)${text.slice(close + 1)}`;
}

function details(signatures, doc) {
  const parts = [];
  if (signatures?.length) parts.push("```python\n" + signatures.map(formatSignature).join("\n") + "\n```");
  if (doc) parts.push(docToMarkdown(doc));
  return parts.join("\n\n");
}

monaco.languages.registerCompletionItemProvider("python", {
  // Names complete as they are typed; these characters start a list without
  // one: attributes, and file names inside strings. Arguments get signature
  // help instead of a list of every name in scope.
  triggerCharacters: [".", '"', "'", "/"],
  async provideCompletionItems(model, position, context, token) {
    const items = await ask("complete", pathOf(model), model.getValue(), position.lineNumber, position.column - 1);
    if (!items || token.isCancellationRequested) return { suggestions: [] };
    return {
      suggestions: items.map((item, index) => ({
        label: item.label,
        kind: KINDS[item.kind] ?? monaco.languages.CompletionItemKind.Text,
        insertText: item.label,
        range: new monaco.Range(
          position.lineNumber,
          Math.max(1, position.column - item.typed),
          position.lineNumber,
          position.column,
        ),
        sortText: String(index).padStart(4, "0"),
        // Jedi ranks them; Monaco must not re-filter by its own scoring.
        filterText: item.label,
        index,
      })),
    };
  },
  async resolveCompletionItem(item) {
    const found = await ask("resolve", item.index, item.label);
    if (found) {
      // The first signature in full, formatted, heads the details; the row
      // keeps Jedi's short description.
      item.detail = found.detail;
      const documentation = details(found.signatures, found.doc);
      if (documentation) item.documentation = { value: documentation };
    }
    return item;
  },
});

monaco.languages.registerHoverProvider("python", {
  async provideHover(model, position) {
    const word = model.getWordAtPosition(position);
    if (!word) return null;
    const found = await ask("hover", pathOf(model), model.getValue(), position.lineNumber, position.column - 1);
    if (!found) return null;
    const contents = [];
    contents.push({ value: "```python\n" + (found.signatures?.length ? found.signatures.map(formatSignature).join("\n") : found.title) + "\n```" });
    if (found.doc) contents.push({ value: docToMarkdown(found.doc) });
    return {
      range: new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, word.endColumn),
      contents,
    };
  },
});

monaco.languages.registerSignatureHelpProvider("python", {
  signatureHelpTriggerCharacters: ["(", ","],
  signatureHelpRetriggerCharacters: [")"],
  async provideSignatureHelp(model, position) {
    const found = await ask("signatures", pathOf(model), model.getValue(), position.lineNumber, position.column - 1);
    if (!found?.length) return null;
    return {
      value: {
        signatures: found.map((signature) => ({
          label: signature.label,
          documentation: signature.doc ? { value: docToMarkdown(signature.doc) } : undefined,
          // The type of the parameter being written, under the signature.
          parameters: signature.params.map((range, index) => ({
            label: range,
            documentation: signature.types?.[index] ? { value: "```python\n" + signature.types[index] + "\n```" } : undefined,
          })),
        })),
        activeSignature: 0,
        activeParameter: found[0].index ?? 0,
      },
      dispose() {},
    };
  },
});

// Syntax errors of the Python file in the editor.
function scheduleSyntaxCheck() {
  clearTimeout(syntaxTimer);
  syntaxTimer = setTimeout(async () => {
    const model = editor.getModel();
    if (!model || model.getLanguageId() !== "python") return;
    const version = model.getVersionId();
    const errors = await ask("errors", pathOf(model), model.getValue());
    if (!errors || model.isDisposed() || version !== model.getVersionId()) return;
    monaco.editor.setModelMarkers(
      model,
      "jedi",
      errors.map((error) => ({
        severity: monaco.MarkerSeverity.Error,
        message: error.message,
        startLineNumber: error.line,
        startColumn: error.column + 1,
        endLineNumber: error.until_line,
        endColumn: error.until_column + 1,
      })),
    );
  }, 450);
}

// The tabs open when the page was last used, and the explorer, now that
// everything they need exists.
openFile(project.has(tabsState.active) ? tabsState.active : ENTRY, { focus: false });

// ---------------------------------------------------------------------------
// Player (embedded)
// ---------------------------------------------------------------------------

let playerReady = false;
let pendingBundle = null;
let lastBundle = null;
// Size and duration of the last recording: { width, height, duration }.
let lastScene = null;
// The GPU passes of the last recording that the light player (and the video
// export) leave out.
let lastFull = [];
// "embed", the light player, or "full", the web player, for a recording
// that needs what the light one leaves out.
let playerKind = null;

function usePlayer(kind) {
  if (kind === playerKind) return;
  playerKind = kind;
  playerReady = false;
  const page = kind === "embed" ? `${config.player}insertar/` : `${config.player}?embed`;
  ui.player.src = new URL(page, location.href).href;
}
usePlayer("embed");

window.addEventListener("message", (event) => {
  if (event.origin !== location.origin) return;
  const message = event.data;
  if (message?.type !== "gaanim:status") return;
  if (event.source === ui.embedPreview.contentWindow) {
    previewStatus(message);
    return;
  }
  if (event.source !== ui.player.contentWindow) return;
  if (message.kind === "ready") {
    playerReady = true;
    if (pendingBundle) openInPlayer(pendingBundle);
    pendingBundle = null;
  } else if (message.kind === "opened" && playerKind === "embed" && message.full?.length) {
    lastFull = message.full;
    // Post-processing or a shader background: the web player draws them.
    usePlayer("full");
    pendingBundle = lastBundle;
  } else if (message.kind === "opened") {
    ui.overlay.hidden = true;
  } else if (message.kind === "error" || message.kind === "damaged") {
    ui.overlay.hidden = true;
    log(`El reproductor no pudo abrir la escena: ${message.message ?? message.kind}`, "error");
  }
});

function openInPlayer(bytes) {
  const copy = bytes.slice(0);
  ui.player.contentWindow.postMessage({ type: "gaanim:open", name: "escena.gaanim", bytes: copy }, location.origin, [copy]);
}

function showBundle(bytes, scene) {
  lastBundle = bytes;
  lastScene = scene;
  lastFull = [];
  ui.downloadBundle.disabled = false;
  ui.downloadVideo.disabled = false;
  ui.embed.disabled = false;
  ui.embed.title = "Insertar: la animación en un blog o sitio web";
  if (ui.embedDialog.open) showEmbedScene();
  // Each recording tries the light player first.
  usePlayer("embed");
  if (playerReady) {
    openInPlayer(bytes);
  } else {
    pendingBundle = bytes;
    ui.overlayText.textContent = "Cargando el reproductor…";
  }
}

// ---------------------------------------------------------------------------
// Runner (runner.js)
// ---------------------------------------------------------------------------

// The runner's worker: { worker, ready, python }.
let runner = null;
let runnerReady = false;
// A second Python, started when a run takes a while: stopping the run swaps
// to it instead of waiting for Python to load again. Not on devices with
// little memory, where two would not fit.
let spare = null;
let spareTimer = 0;
const SPARE = (navigator.deviceMemory ?? 8) >= 4;
// The run in progress, as the overlay tells it: when it started, its phase
// and how far the recording went ({ done, total }).
let runStarted = 0;
let runPhase = "script";
let recorded = null;
let runTicker = 0;
// The frame rate the run records at.
let runFps = 30;
let running = null;
let runCounter = 0;
let firstRun = true;
// A run asked for while another runs (saving twice): it starts when that ends.
let rerun = false;
// Assets the runner has, by path, with the `modified` it got; and whether an
// asset changed or went since the last run (the engine drops its caches).
let sentAssets = new Map();
let assetsDirty = true;

function makeRunner() {
  const entry = { worker: new Worker(`runner.js?v=${config.version}`, { type: "module" }), ready: false, python: null };
  entry.worker.onmessage = (event) => onRunnerMessage(entry, event);
  entry.worker.onerror = (event) => {
    if (entry === runner) setStatus(`El motor falló: ${event.message ?? "error desconocido"}`, "error");
    else if (entry === spare) spare = null;
  };
  entry.worker.postMessage({ type: "init", config });
  return entry;
}

// Use `entry` (a spare, or a new one) as the runner.
function useRunner(entry = makeRunner()) {
  runner = entry;
  runnerReady = entry.ready;
  // A new Python has none of the assets.
  sentAssets = new Map();
  assetsDirty = true;
  ui.run.disabled = !runnerReady;
}

function setRunning(active) {
  ui.run.disabled = active || !runnerReady;
  ui.stop.hidden = !active;
  ui.overlay.hidden = !active;
  if (!active) {
    clearInterval(runTicker);
    clearTimeout(spareTimer);
    ui.progress.hidden = true;
    ui.overlayHint.hidden = true;
  }
}

// The overlay while a run goes: its phase, how far the recording went and
// the time it has taken; after a while, how to stop it.
function showRunState() {
  if (running === null) return;
  const seconds = Math.floor((performance.now() - runStarted) / 1000);
  const share = runPhase === "record" && recorded?.total ? recorded.done / recorded.total : null;
  let text = runPhase === "record" ? "Grabando la escena…" : "Ejecutando el script…";
  if (share !== null) text = `Grabando la escena… ${Math.floor(share * 100)} %`;
  if (seconds >= 1) text += ` · ${seconds} s`;
  ui.overlayText.textContent = text;
  ui.overlayHint.textContent = rerun ? "Se ejecutará de nuevo al terminar · Esc detiene" : "Esc o Detener la paran";
  ui.overlayHint.hidden = !rerun && seconds < 8;
  ui.progress.hidden = share === null;
  if (share !== null) ui.progress.value = share;
}

function onRunnerMessage(entry, { data }) {
  if (entry !== runner) {
    // The spare only gets ready, or fails to.
    if (data.type === "ready") {
      entry.ready = true;
      entry.python = data.python;
    } else if (data.type === "fatal") {
      entry.worker.terminate();
      if (spare === entry) spare = null;
    }
    return;
  }
  switch (data.type) {
    case "progress":
      setStatus(data.text, "busy");
      break;
    case "ready":
      entry.ready = true;
      entry.python = data.python;
      runnerReady = true;
      ui.run.disabled = false;
      startLanguage();
      setStatus("Listo. Ctrl+S guarda y ejecuta la escena.", "ready");
      ui.versions.textContent = `gaanim ${config.version} · Python ${data.python ?? "3.14"} · Pyodide ${data.pyodide ?? ""}`;
      if (firstRun || rerun) {
        firstRun = false;
        rerun = false;
        run();
      }
      break;
    case "stdout":
      log(data.text);
      break;
    case "stderr":
      log(data.text, "stderr");
      break;
    case "info":
      log(data.text, "info");
      break;
    case "phase":
      if (data.id === running && data.phase === "record") {
        if (runPhase !== "record") setStatus("Grabando la escena…", "busy");
        runPhase = "record";
        recorded = { done: data.done ?? 0, total: data.total ?? 0 };
        showRunState();
      }
      break;
    case "done":
      if (data.id !== running) break;
      running = null;
      setRunning(false);
      ui.overlay.hidden = playerReady;
      showBundle(data.bytes, { ...data.scene, fps: runFps });
      log(
        `✓ Escena lista en ${data.timings.total.toFixed(1)} s (script ${data.timings.run.toFixed(1)} s, grabación ${data.timings.record.toFixed(1)} s, ${formatSize(data.bytes.byteLength)}).`,
        "ok",
      );
      setStatus("Listo.", "ready");
      runQueued();
      break;
    case "failed":
      if (data.id !== running) break;
      running = null;
      setRunning(false);
      showFailure(data.error);
      setStatus(
        data.error.phase === "record" ? "La grabación falló." : data.error.phase === "install" ? "No se pudieron instalar los paquetes." : "El script falló.",
        "error",
      );
      runQueued();
      break;
    case "fatal":
      setStatus(`No se pudo iniciar el motor: ${data.error}`, "error");
      log(`No se pudo iniciar el motor: ${data.error}`, "error");
      break;
  }
}

// Where the project lists its packages: the [project] dependencies of its
// pyproject.toml, where `uv add` writes in an application project, or
// requirements.txt.
function requirementsFile() {
  const pyproject = project.get("pyproject.toml");
  if (pyproject?.text !== undefined && addDependency(models.get("pyproject.toml")?.getValue() ?? pyproject.text, "x") !== null) {
    return "pyproject.toml";
  }
  return "requirements.txt";
}

// A module the project imports and does not have: maybe a package of PyPI,
// which the project's list installs.
function packageHint(error) {
  const name = /^No module named '([^'.]+)/.exec(error.message ?? "")?.[1];
  if (error.kind !== "ModuleNotFoundError" || !name || exists(`${name}.py`) || exists(name)) return;
  const file = requirementsFile();
  log(
    `Si «${name}» es un paquete de PyPI, añádelo a ${file} (a veces se llama distinto de su import) y vuelve a ejecutar: se instala en el navegador si es de Python puro o Pyodide lo compila.`,
    "hint",
    { label: `Añadir «${name}» a ${file}`, run: () => addRequirement(name) },
  );
}

// `toml` with `name` in its [project] dependencies, written as uv writes
// them; null when it has no such array to add to.
function addDependency(toml, name) {
  const section = /^\[project\][ \t]*$/m.exec(toml);
  if (!section) return null;
  const after = toml.slice(section.index + section[0].length);
  const next = /^\[/m.exec(after);
  const end = section.index + section[0].length + (next ? next.index : after.length);
  const body = toml.slice(section.index, end);
  const array = /^(dependencies\s*=\s*\[)([^\]]*)\]/m.exec(body);
  if (!array) return null;
  const items = array[2];
  const listed = items.match(/["']([A-Za-z0-9._-]+)/g)?.map((item) => item.slice(1).toLowerCase()) ?? [];
  if (listed.includes(name.toLowerCase())) return toml;
  let replaced;
  if (items.includes("\n")) {
    // One per line, with a trailing comma.
    const kept = items.replace(/\s*$/, "");
    const comma = !kept.trim() || kept.endsWith(",") ? "" : ",";
    replaced = `${array[1]}${kept}${comma}\n    "${name}",\n]`;
  } else {
    const kept = items.trim().replace(/,$/, "");
    replaced = `${array[1]}${kept ? `${kept}, ` : ""}"${name}"]`;
  }
  return toml.slice(0, section.index) + body.replace(array[0], replaced) + toml.slice(end);
}

function addRequirement(name) {
  const path = requirementsFile();
  const current = models.get(path)?.getValue() ?? project.get(path)?.text ?? "";
  let text;
  if (path === "pyproject.toml") {
    text = addDependency(current, name);
  } else {
    const lines = current.split("\n").map((line) => line.trim());
    text = lines.includes(name) ? current : `${current.replace(/\s*$/, "")}${current.trim() ? "\n" : ""}${name}\n`;
  }
  if (text !== current) setFile({ path, text });
  openFile(path);
  run();
}

// Show a failure in the console and mark its line in its file.
function showFailure(error) {
  if (error.traceback) log(error.traceback.trimEnd(), "error");
  else log(`${error.kind}: ${error.message}`, "error");
  packageHint(error);
  const path = error.file && project.get(error.file)?.text !== undefined ? error.file : null;
  if (!path || !error.line) return;
  openFile(path, { focus: false });
  const model = modelFor(path);
  const line = Math.min(error.line, model.getLineCount());
  monaco.editor.setModelMarkers(model, "runtime", [
    {
      severity: monaco.MarkerSeverity.Error,
      message: `${error.kind}: ${error.message}`,
      startLineNumber: line,
      startColumn: model.getLineFirstNonWhitespaceColumn(line) || 1,
      endLineNumber: line,
      endColumn: model.getLineMaxColumn(line),
    },
  ]);
  errorMarks.set(
    path,
    model.deltaDecorations(errorMarks.get(path) ?? [], [
      {
        range: new monaco.Range(line, 1, line, 1),
        options: { isWholeLine: true, className: "gaanim-error-line", glyphMarginClassName: "gaanim-error-glyph" },
      },
    ]),
  );
  editor.revealLineInCenterIfOutsideViewport(line);
}

function run() {
  if (running !== null) {
    rerun = true;
    showRunState();
    return;
  }
  if (!runnerReady) {
    rerun = true;
    toast("Python todavía está cargando: la escena se ejecutará en cuanto esté listo.");
    return;
  }
  flushSaves();
  running = ++runCounter;
  // Every text file goes each time (they are small); assets only when the
  // runner does not have them as they are.
  const files = [];
  const imports = [];
  const assets = new Map();
  for (const file of project.values()) {
    if (file.text !== undefined) {
      const text = models.get(file.path)?.getValue() ?? file.text;
      files.push({ path: file.path, text });
      if (kindOf(file.path) === "python") imports.push(text);
    } else {
      assets.set(file.path, file.modified);
      if (sentAssets.get(file.path) !== file.modified) files.push({ path: file.path, bytes: file.bytes });
    }
  }
  const assetsChanged = assetsDirty || [...sentAssets.keys()].some((path) => !assets.has(path));
  sentAssets = assets;
  assetsDirty = false;
  const message = {
    type: "run",
    id: running,
    files,
    paths: [...project.keys()],
    folders: [...folders],
    imports: imports.join("\n"),
    fps: Number(ui.fps.value),
    assetsChanged,
  };
  runFps = message.fps;
  for (const path of models.keys()) clearRuntimeError(path);
  fontHinted = false;
  runStarted = performance.now();
  runPhase = "script";
  recorded = null;
  setRunning(true);
  showRunState();
  clearInterval(runTicker);
  runTicker = setInterval(showRunState, 250);
  // A run that takes a while warms a spare Python for Stop.
  clearTimeout(spareTimer);
  if (SPARE && !spare) spareTimer = setTimeout(() => running !== null && !spare && (spare = makeRunner()), 2000);
  setStatus("Ejecutando…", "busy");
  log("▶ main.py", "info");
  runner.worker.postMessage(message);
}

// Live mode: run a moment after typing stops, when the Python files parse
// (a half-typed line would only fail).
function scheduleLiveRun() {
  if (!live) return;
  clearTimeout(liveTimer);
  liveTimer = setTimeout(async () => {
    for (const [path, model] of models) {
      if (model.getLanguageId() !== "python") continue;
      const errors = await ask("errors", path, model.getValue());
      if (errors?.length) return;
    }
    run();
  }, 900);
}

function runQueued() {
  if (!rerun) return;
  rerun = false;
  run();
}

function stop() {
  if (running === null) return;
  runner.worker.terminate();
  running = null;
  rerun = false;
  setRunning(false);
  const next = spare;
  spare = null;
  useRunner(next ?? undefined);
  if (runnerReady) {
    log("■ Detenido.", "info");
    setStatus("Listo.", "ready");
  } else {
    log("■ Detenido. Python se reinicia.", "info");
    setStatus("Python se reinicia…", "busy");
  }
}

ui.run.addEventListener("click", run);
ui.stop.addEventListener("click", stop);
useRunner();

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

function showLive() {
  ui.live.setAttribute("aria-pressed", String(live));
  ui.live.title = live ? "En vivo: la escena se ejecuta al dejar de escribir. Pulsa para apagarlo." : "Ejecutar la escena al dejar de escribir";
}
live = load(KEYS.live) === "1";
showLive();
ui.live.addEventListener("click", () => {
  live = !live;
  store(KEYS.live, live ? "1" : "0");
  showLive();
  if (live) scheduleLiveRun();
  else clearTimeout(liveTimer);
});

ui.fps.value = load(KEYS.fps) ?? "30";
ui.fps.addEventListener("change", () => store(KEYS.fps, ui.fps.value));

ui.share.addEventListener("click", async () => {
  flushSaves();
  const files = {};
  let assets = 0;
  for (const file of project.values()) {
    if (file.text !== undefined) files[file.path] = models.get(file.path)?.getValue() ?? file.text;
    else assets++;
  }
  const single = Object.keys(files).length === 1;
  const fragment = single ? `c=${await compress(files[ENTRY])}` : `p=${await compress(JSON.stringify({ files }))}`;
  const url = `${location.origin}${location.pathname}#${fragment}`;
  history.replaceState(null, "", url);
  try {
    await navigator.clipboard.writeText(url);
    toast(assets ? "Enlace copiado. Lleva los archivos de texto; las imágenes, el audio y las fuentes no viajan en él." : "Enlace copiado.");
  } catch {
    toast("El enlace está en la barra de direcciones.");
  }
});

function closeMenu() {
  ui.downloadMenu.hidden = true;
  ui.downloadToggle.setAttribute("aria-expanded", "false");
}
ui.downloadToggle.addEventListener("click", (event) => {
  event.stopPropagation();
  const open = ui.downloadMenu.hidden;
  ui.downloadBundle.textContent = `${embedName()}.gaanim`;
  ui.downloadMenu.hidden = !open;
  ui.downloadToggle.setAttribute("aria-expanded", String(open));
});
document.addEventListener("click", closeMenu);
ui.downloadProject.addEventListener("click", downloadProject);
ui.downloadBundle.addEventListener("click", downloadRecording);
ui.downloadVideo.addEventListener("click", openVideo);

function downloadRecording() {
  if (lastBundle) downloadBlob(`${embedName()}.gaanim`, lastBundle);
}

// ---------------------------------------------------------------------------
// Insertar: the recording in a blog or website
// ---------------------------------------------------------------------------

// The page embeds the embed player in an <iframe> that reads the .gaanim
// file from the site that publishes it (`insertar/?src=…`). How it plays,
// and the file's name and address, are remembered for the next one.
const embed = (() => {
  const saved = loadJson(KEYS.embed, {});
  return {
    // Empty: the project's name.
    name: typeof saved.name === "string" ? saved.name : "",
    url: typeof saved.url === "string" ? saved.url : "",
    loop: saved.loop ?? true,
    autoplay: saved.autoplay ?? true,
    controls: saved.controls ?? false,
  };
})();
const playerUrl = new URL(config.player, location.href);
// The embed player, which the code and the preview use.
const embedUrl = new URL("insertar/", playerUrl);
const EXAMPLE_FOLDER = "https://tu-blog.com/animaciones/";
let previewReady = false;
// The recording the preview shows.
let previewBundle = null;

function saveEmbed() {
  store(KEYS.embed, JSON.stringify(embed));
}

// The file's name as sites like it in addresses: lowercase, without accents,
// spaces, signs or the extension.
function embedName() {
  const name = embed.name
    .trim()
    .replace(/\.gaanim$/i, "")
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9._-]+/g, "-")
    .replace(/^[-.]+|[-.]+$/g, "");
  return name || slug(currentName(), "animacion");
}

// The address the file will have: the one typed, inside the folder typed,
// or an example of one.
function fileUrl() {
  const typed = embed.url.trim();
  const name = `${embedName()}.gaanim`;
  if (!typed) return EXAMPLE_FOLDER + name;
  if (/\.gaanim$/i.test(typed)) return typed;
  return typed.replace(/\/*$/, "/") + name;
}

// The player's address for `src`. The file's address stays readable: only
// what would end it, or the attribute that holds it, is escaped.
function embedSource(url) {
  const params = [`src=${url.replace(/[%&#+ "<>]/g, encodeURIComponent)}`];
  if (embed.loop) params.push("loop");
  if (!embed.controls) params.push("controls=0");
  if (!embed.autoplay) params.push("autoplay=0");
  return `${embedUrl.href}?${params.join("&")}`;
}

function aspectRatio(scene) {
  if (!scene?.width || !scene?.height) return "16 / 9";
  const gcd = (a, b) => (b ? gcd(b, a % b) : a);
  const divisor = gcd(scene.width, scene.height);
  return `${scene.width / divisor} / ${scene.height / divisor}`;
}

function escapeHtml(text) {
  return text.replace(/[&<>"]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[char]);
}

function embedCode() {
  const title = `Animación: ${embedName().replace(/[-_]+/g, " ")}`;
  return [
    `<iframe src="${escapeHtml(embedSource(fileUrl()))}"`,
    `  title="${escapeHtml(title)}" loading="lazy" allow="fullscreen; clipboard-write"`,
    `  style="width: 100%; aspect-ratio: ${aspectRatio(lastScene)}; border: 0"></iframe>`,
  ].join("\n");
}

function absoluteUrl(text) {
  try {
    return ["http:", "https:"].includes(new URL(text).protocol);
  } catch {
    return false;
  }
}

function showEmbed() {
  const name = `${embedName()}.gaanim`;
  ui.downloadBundle.textContent = name;
  const typed = embed.url.trim();
  const valid = !typed || absoluteUrl(typed);
  ui.embedUrl.classList.toggle("invalid", !valid);
  ui.embedUrl.setCustomValidity(valid ? "" : "Escribe la dirección completa, con https://");
  ui.embedUrl.placeholder = EXAMPLE_FOLDER;
  // The embed player opens the published file, or says why it cannot.
  ui.embedTestWrap.hidden = !(typed && valid);
  if (typed && valid) {
    ui.embedTest.href = embedSource(fileUrl());
  }
  ui.embedCode.value = embedCode();
}

function showEmbedScene() {
  ui.embedFrame.style.aspectRatio = aspectRatio(lastScene);
  if (lastScene) {
    const seconds = lastScene.duration.toLocaleString("es", { maximumFractionDigits: 1 });
    ui.embedMeta.textContent = `${lastScene.width} × ${lastScene.height} · ${seconds} s · ${formatSize(lastBundle.byteLength)}`;
  }
  showEmbed();
  sendToPreview();
}

function previewOptions() {
  return { loop: embed.loop, controls: embed.controls, autoplay: embed.autoplay };
}

function sendToPreview() {
  if (!previewReady || !lastBundle || previewBundle === lastBundle) return;
  previewBundle = lastBundle;
  const copy = lastBundle.slice(0);
  ui.embedPreview.contentWindow.postMessage(
    { type: "gaanim:open", name: `${embedName()}.gaanim`, bytes: copy },
    location.origin,
    [copy],
  );
}

function previewStatus(message) {
  if (message.kind === "ready") {
    previewReady = true;
    sendToPreview();
  } else if (message.kind === "error" || message.kind === "damaged") {
    log(`La vista previa no pudo abrir la escena: ${message.message ?? message.kind}`, "error");
  }
}

function openEmbed() {
  if (!lastBundle) return;
  closeMenu();
  ui.embedName.value = embedName();
  ui.embedUrl.value = embed.url;
  ui.embedLoop.checked = embed.loop;
  ui.embedAutoplay.checked = embed.autoplay;
  ui.embedControls.checked = embed.controls;
  ui.embedLocal.hidden = !["localhost", "127.0.0.1", "[::1]"].includes(embedUrl.hostname);
  // The preview is a second player, started the first time it is needed.
  if (!ui.embedPreview.src) {
    const params = new URLSearchParams();
    if (embed.loop) params.set("loop", "1");
    if (!embed.controls) params.set("controls", "0");
    if (!embed.autoplay) params.set("autoplay", "0");
    ui.embedPreview.src = `${embedUrl.href}?${params}`;
  } else if (previewReady) {
    ui.embedPreview.contentWindow.postMessage({ type: "gaanim:options", ...previewOptions() }, location.origin);
  }
  ui.embedDialog.showModal();
  showEmbedScene();
}

ui.embed.addEventListener("click", openEmbed);
ui.embedClose.addEventListener("click", () => ui.embedDialog.close());
// A click on the backdrop closes it too.
ui.embedDialog.addEventListener("click", (event) => {
  if (event.target === ui.embedDialog) ui.embedDialog.close();
});
ui.embedName.addEventListener("input", () => {
  embed.name = ui.embedName.value;
  saveEmbed();
  showEmbed();
});
ui.embedName.addEventListener("change", () => {
  ui.embedName.value = embedName();
});
ui.embedUrl.addEventListener("input", () => {
  embed.url = ui.embedUrl.value;
  saveEmbed();
  showEmbed();
});
for (const [box, key] of [
  [ui.embedLoop, "loop"],
  [ui.embedAutoplay, "autoplay"],
  [ui.embedControls, "controls"],
]) {
  box.addEventListener("change", () => {
    embed[key] = box.checked;
    saveEmbed();
    showEmbed();
    if (previewReady) {
      ui.embedPreview.contentWindow.postMessage({ type: "gaanim:options", ...previewOptions() }, location.origin);
    }
  });
}
ui.embedDownload.addEventListener("click", downloadRecording);
ui.embedCopy.addEventListener("click", async () => {
  const code = embedCode();
  try {
    await navigator.clipboard.writeText(code);
    toast(embed.url.trim() ? "Código copiado." : "Código copiado. Cambia la dirección de ejemplo por la de tu archivo.");
  } catch {
    ui.embedCode.focus();
    ui.embedCode.select();
    toast("Copia el código seleccionado.");
  }
});
showEmbed();

document.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
    event.preventDefault();
    run();
  } else if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
    event.preventDefault();
    saveAndRun();
  } else if (event.key === "Escape" && running !== null && !ui.embedDialog.open && !ui.videoDialog.open && ui.contextMenu.hidden && !editing) {
    stop();
  }
});

// ---------------------------------------------------------------------------
// Video: the recording as MP4, WebM or GIF (video.js)
// ---------------------------------------------------------------------------

const video = { format: "mp4", height: 1080, ...loadJson(KEYS.video, {}) };
// The export in progress, to cancel it.
let videoAbort = null;
// What the GPU passes are called on the page.
const PASSES = { post: "el posprocesado", background: "el fondo de shader", transition: "las transiciones de shader", effect: "los efectos de shader" };

function listOf(items) {
  return items.length > 1 ? `${items.slice(0, -1).join(", ")} y ${items.at(-1)}` : (items[0] ?? "");
}

// The size, length and warnings of the export as chosen.
async function showVideoOptions() {
  const format = ui.videoFormat.value;
  const size = evenSize(lastScene, Number(ui.videoHeight.value));
  const recorded = lastScene?.fps ?? Number(ui.fps.value);
  const fps = format === "gif" ? Math.min(recorded, 25) : recorded;
  const seconds = lastScene?.duration ?? 0;
  ui.videoMeta.textContent = `${size.width} × ${size.height} px · ${seconds.toFixed(1)} s · ${Math.max(1, Math.ceil(seconds * fps))} fotogramas`;
  const warnings = [];
  if (lastFull.length) {
    warnings.push(`La escena usa ${listOf(lastFull.map((pass) => PASSES[pass] ?? pass))}, que el vídeo del navegador no dibuja. La aplicación de escritorio exporta la escena entera.`);
  }
  if (format === "gif") {
    warnings.push(`GIF: hasta 25 fotogramas por segundo, 256 colores y sin sonido; pesa bastante más que un vídeo${size.height > 720 ? ", sobre todo a este tamaño" : ""}.`);
  }
  const codec = videoAbort ? true : await supportedCodec(format, size.width, size.height, fps);
  if (!codec) warnings.push(`Este navegador no codifica ${FORMATS[format].label} a ${size.width} × ${size.height}. Prueba otro formato o un tamaño menor.`);
  ui.videoStart.disabled = !codec || Boolean(videoAbort);
  ui.videoWarning.hidden = !warnings.length;
  ui.videoWarning.textContent = warnings.join(" ");
}

function setVideoBusy(busy) {
  ui.videoProgress.hidden = !busy;
  ui.videoCancel.hidden = !busy;
  ui.videoStart.disabled = busy;
  ui.videoFormat.disabled = busy;
  ui.videoHeight.disabled = busy;
}

function openVideo() {
  if (!lastBundle) return;
  closeMenu();
  ui.videoFormat.value = video.format;
  ui.videoHeight.value = String(video.height);
  if (!ui.videoHeight.value) ui.videoHeight.value = "1080";
  if (!videoAbort) ui.videoStatus.textContent = "";
  ui.videoDialog.showModal();
  showVideoOptions();
}

async function startVideo() {
  if (!lastBundle || videoAbort) return;
  const format = ui.videoFormat.value;
  const spec = FORMATS[format];
  const name = `${embedName()}.${spec.extension}`;
  videoAbort = new AbortController();
  setVideoBusy(true);
  ui.videoProgress.value = 0;
  try {
    const result = await exportVideo({
      bundle: lastBundle,
      scene: lastScene,
      format,
      height: Number(ui.videoHeight.value),
      playerUrl,
      signal: videoAbort.signal,
      onProgress(fraction, text) {
        ui.videoProgress.value = fraction;
        ui.videoStatus.textContent = text;
      },
    });
    downloadBlob(name, result.blob, spec.type);
    let text = `${name} descargado (${formatSize(result.blob.size)}).`;
    if (result.sound && format === "gif") text += " El GIF no lleva el sonido de la escena.";
    else if (result.sound && !result.audio) text += " Va sin sonido: este navegador no codifica el audio de este formato.";
    ui.videoStatus.textContent = text;
  } catch (error) {
    ui.videoStatus.textContent = error?.name === "AbortError" ? "Exportación cancelada." : `No se pudo exportar: ${error?.message ?? error}`;
  } finally {
    videoAbort = null;
    setVideoBusy(false);
    showVideoOptions();
  }
}

for (const [select, key] of [
  [ui.videoFormat, "format"],
  [ui.videoHeight, "height"],
]) {
  select.addEventListener("change", () => {
    video[key] = key === "height" ? Number(select.value) : select.value;
    store(KEYS.video, JSON.stringify(video));
    showVideoOptions();
  });
}
ui.videoStart.addEventListener("click", startVideo);
ui.videoCancel.addEventListener("click", () => videoAbort?.abort());
ui.videoClose.addEventListener("click", () => ui.videoDialog.close());
ui.videoDialog.addEventListener("click", (event) => {
  if (event.target === ui.videoDialog) ui.videoDialog.close();
});
// Closing the dialog cancels the export.
ui.videoDialog.addEventListener("close", () => videoAbort?.abort());

// ---------------------------------------------------------------------------
// Splitters
// ---------------------------------------------------------------------------

const layout = loadJson(KEYS.layout, {});
const root = document.documentElement.style;
if (layout.side) root.setProperty("--side", `${layout.side}%`);
if (layout.player) root.setProperty("--player", `${layout.player}%`);

function dragSplitter(splitter, axis) {
  splitter.addEventListener("pointerdown", (event) => {
    event.preventDefault();
    splitter.setPointerCapture(event.pointerId);
    splitter.classList.add("dragging");
    // The iframe would swallow pointer events while dragging over it.
    ui.player.style.pointerEvents = "none";
    const area = axis === "x" ? $("layout").getBoundingClientRect() : $("side").getBoundingClientRect();
    const move = (moveEvent) => {
      if (axis === "x") {
        const side = ((area.right - moveEvent.clientX) / area.width) * 100;
        layout.side = Math.min(75, Math.max(22, side));
        root.setProperty("--side", `${layout.side}%`);
      } else {
        const player = ((moveEvent.clientY - area.top) / area.height) * 100;
        layout.player = Math.min(85, Math.max(15, player));
        root.setProperty("--player", `${layout.player}%`);
      }
    };
    const up = () => {
      splitter.classList.remove("dragging");
      ui.player.style.pointerEvents = "";
      splitter.removeEventListener("pointermove", move);
      store(KEYS.layout, JSON.stringify(layout));
    };
    splitter.addEventListener("pointermove", move);
    splitter.addEventListener("pointerup", up, { once: true });
    splitter.addEventListener("pointercancel", up, { once: true });
  });
}
dragSplitter($("split-x"), "x");
dragSplitter($("split-y"), "y");

if (!navigator.gpu) {
  log("Este navegador no tiene WebGPU, que el reproductor necesita: el código se ejecuta, pero no se verá la escena. Usa una versión reciente de Chrome, Edge, Safari o Firefox.", "stderr");
}
