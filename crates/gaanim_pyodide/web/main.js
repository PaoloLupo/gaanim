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
  examples: $("examples"),
  downloadToggle: $("download-toggle"),
  downloadMenu: $("download-menu"),
  downloadProject: $("download-project"),
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
  console: $("console"),
  clearConsole: $("clear-console"),
  status: $("status"),
  statusDot: $("status-dot"),
  progress: $("progress"),
  versions: $("versions"),
  toast: $("toast"),
  dropping: $("dropping"),
  embed: $("embed"),
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

function log(text, kind = "") {
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
  const atBottom = ui.console.scrollHeight - ui.console.scrollTop - ui.console.clientHeight < 40;
  ui.console.append(line);
  if (atBottom) ui.console.scrollTop = ui.console.scrollHeight;
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

const examples = await fetch(config.examples)
  .then((response) => response.json())
  .catch(() => []);
for (const example of examples) {
  ui.examples.append(new Option(example.title, example.name));
}

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
// Storage: the project in IndexedDB
// ---------------------------------------------------------------------------

// Version 1 kept uploads by name in "files"; version 2 keeps the project by
// path in "project" (folders as records with `folder: true`) and moves the
// uploads to its root, where scripts found them.
const database = await new Promise((resolve) => {
  try {
    const request = indexedDB.open("gaanim-playground", 2);
    request.onupgradeneeded = (event) => {
      const db = request.result;
      const fresh = db.objectStoreNames.contains("project")
        ? request.transaction.objectStore("project")
        : db.createObjectStore("project", { keyPath: "path" });
      if (event.oldVersion >= 1 && db.objectStoreNames.contains("files")) {
        request.transaction.objectStore("files").openCursor().onsuccess = ({ target }) => {
          const cursor = target.result;
          if (!cursor) return;
          const record = cursor.value;
          const path = cleanPath(record.name ?? "");
          if (path) fresh.put({ path, bytes: record.bytes, type: record.type, size: record.size, modified: record.modified });
          cursor.continue();
        };
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
      const tx = database.transaction("project", mode);
      const result = work(tx.objectStore("project"));
      tx.oncomplete = () => resolve(result?.result ?? null);
      tx.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

const saveTimers = new Map();
// Keep `path` as it is now; text edits wait a moment, so typing does not
// write on every key.
function persist(path, { soon = false } = {}) {
  clearTimeout(saveTimers.get(path));
  const write = () => {
    saveTimers.delete(path);
    const file = project.get(path);
    if (file) transaction("readwrite", (s) => s.put(file));
    else if (folders.has(path)) transaction("readwrite", (s) => s.put({ path, folder: true }));
    else transaction("readwrite", (s) => s.delete(path));
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

for (const record of (await transaction("readonly", (s) => s.getAll())) ?? []) {
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

// A shared link brings its files; a project without main.py starts from the
// code of the playground before projects, or from the first example.
const shared = await sharedFiles();
for (const [path, text] of Object.entries(shared ?? {})) {
  project.set(path, { path, text });
  persist(path);
}
if (!project.has(ENTRY)) {
  project.set(ENTRY, { path: ENTRY, text: load(KEYS.code) ?? examples[0]?.code ?? FALLBACK_CODE });
  persist(ENTRY);
}

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

// The brand's palette (docs/assets/brand/README.md): graphite, violet, and
// gold for the current frame, here the cursor.
monaco.editor.defineTheme("gaanim", {
  base: "vs-dark",
  inherit: true,
  rules: [
    { token: "comment", foreground: "6b6b73", fontStyle: "italic" },
    { token: "keyword", foreground: "a99fff" },
    { token: "string", foreground: "cbc6ff" },
    { token: "number", foreground: "ffc933" },
    { token: "delimiter", foreground: "a0a0a0" },
  ],
  colors: {
    "editor.background": "#121212",
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
    "editorBracketHighlight.foreground3": "#cbc6ff",
    "focusBorder": "#a99fff",
  },
});

ui.editor.textContent = "";
const editor = monaco.editor.create(ui.editor, {
  model: null,
  theme: "gaanim",
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

const tabsState = loadJson(KEYS.tabs, {});
let openTabs = (Array.isArray(tabsState.open) ? tabsState.open : []).filter((path) => project.has(path));
if (!openTabs.includes(ENTRY)) openTabs.unshift(ENTRY);
let activePath = null;
let assetUrl = null;

function saveTabs() {
  store(KEYS.tabs, JSON.stringify({ open: openTabs, active: activePath }));
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
    const icon = document.createElement("span");
    icon.className = `file-icon ${kindOf(path)}`;
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
    const icon = document.createElement("span");
    icon.className = `file-icon big ${kind}`;
    stage.append(icon);
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

const expanded = new Set(loadJson(KEYS.expanded, []));
let selectedPath = null;
// The row being renamed or created: { path, folder, kind } while it edits.
let editing = null;

function saveExpanded() {
  store(KEYS.expanded, JSON.stringify([...expanded]));
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
  const icon = document.createElement("span");
  icon.className = `file-icon ${folder ? "folder" : kindOf(path)}`;
  item.append(chevron, icon);
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
  const icon = document.createElement("span");
  icon.className = `file-icon ${editing.kind === "folder" ? "folder" : "text"}`;
  item.append(chevron, icon, renameInput(""));
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
    ...items.map(({ label, run: action, danger }) => {
      if (!label) return Object.assign(document.createElement("hr"), {});
      const button = Object.assign(document.createElement("button"), { type: "button", textContent: label });
      button.setAttribute("role", "menuitem");
      if (danger) button.className = "danger";
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

function downloadProject() {
  flushSaves();
  const encoder = new TextEncoder();
  const entries = [...project.values()]
    .sort((a, b) => byName(a.path, b.path))
    .map((file) => ({
      path: `proyecto/${file.path}`,
      bytes: file.text !== undefined ? encoder.encode(models.get(file.path)?.getValue() ?? file.text) : new Uint8Array(file.bytes),
    }));
  downloadBlob("proyecto-gaanim.zip", zipArchive(entries), "application/zip");
}

// ---------------------------------------------------------------------------
// Language services (language.js)
// ---------------------------------------------------------------------------

const language = new Worker("language.js", { type: "module" });
let requestId = 0;
const waiting = new Map();

language.onmessage = ({ data }) => {
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
};
language.postMessage({ type: "init", config });

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

function details(signatures, doc) {
  const parts = [];
  if (signatures?.length) parts.push("```python\n" + signatures.join("\n") + "\n```");
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
      item.detail = found.signatures?.[0] ?? found.detail;
      const documentation = details(found.signatures?.slice(1), found.doc);
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
    contents.push({ value: "```python\n" + (found.signatures?.length ? found.signatures.join("\n") : found.title) + "\n```" });
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
          parameters: signature.params.map((range) => ({ label: range })),
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
  ui.downloadBundle.disabled = false;
  ui.embed.disabled = false;
  ui.embed.title = "Insertar la animación en un blog o sitio web";
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

let runner = null;
let runnerReady = false;
let running = null;
let runCounter = 0;
let firstRun = true;
// A run asked for while another runs (saving twice): it starts when that ends.
let rerun = false;
// Assets the runner has, by path, with the `modified` it got; and whether an
// asset changed or went since the last run (the engine drops its caches).
let sentAssets = new Map();
let assetsDirty = true;

function startRunner() {
  runnerReady = false;
  sentAssets = new Map();
  assetsDirty = true;
  ui.run.disabled = true;
  runner = new Worker("runner.js", { type: "module" });
  runner.onmessage = onRunnerMessage;
  runner.onerror = (event) => {
    setStatus(`El motor falló: ${event.message ?? "error desconocido"}`, "error");
  };
  runner.postMessage({ type: "init", config });
}

function setRunning(active) {
  ui.run.disabled = active || !runnerReady;
  ui.stop.hidden = !active;
  ui.overlay.hidden = !active;
}

function onRunnerMessage({ data }) {
  switch (data.type) {
    case "progress":
      setStatus(data.text, "busy");
      break;
    case "ready":
      runnerReady = true;
      ui.run.disabled = false;
      setStatus("Listo. Ctrl+S guarda y ejecuta la escena.", "ready");
      ui.versions.textContent = `gaanim ${config.version} · Python ${data.python ?? "3.14"}`;
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
        ui.overlayText.textContent = "Grabando la escena…";
        setStatus("Grabando la escena…", "busy");
      }
      break;
    case "done":
      if (data.id !== running) break;
      running = null;
      setRunning(false);
      ui.overlay.hidden = playerReady;
      showBundle(data.bytes, data.scene);
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
      setStatus(data.error.phase === "record" ? "La grabación falló." : "El script falló.", "error");
      runQueued();
      break;
    case "fatal":
      setStatus(`No se pudo iniciar el motor: ${data.error}`, "error");
      log(`No se pudo iniciar el motor: ${data.error}`, "error");
      break;
  }
}

// Show a failure in the console and mark its line in its file.
function showFailure(error) {
  if (error.traceback) log(error.traceback.trimEnd(), "error");
  else log(`${error.kind}: ${error.message}`, "error");
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
    ui.overlayText.textContent = "Se ejecutará de nuevo al terminar…";
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
  for (const path of models.keys()) clearRuntimeError(path);
  ui.overlayText.textContent = "Ejecutando el script…";
  setRunning(true);
  setStatus("Ejecutando…", "busy");
  log("▶ main.py", "info");
  runner.postMessage(message);
}

function runQueued() {
  if (!rerun) return;
  rerun = false;
  run();
}

function stop() {
  if (running === null) return;
  runner.terminate();
  running = null;
  rerun = false;
  setRunning(false);
  log("■ Detenido. Python se reinicia.", "info");
  startRunner();
}

ui.run.addEventListener("click", run);
ui.stop.addEventListener("click", stop);
startRunner();

// ---------------------------------------------------------------------------
// Toolbar
// ---------------------------------------------------------------------------

ui.fps.value = load(KEYS.fps) ?? "30";
ui.fps.addEventListener("change", () => store(KEYS.fps, ui.fps.value));

let loadedExample = project.get(ENTRY).text;
ui.examples.addEventListener("change", () => {
  const example = examples.find((entry) => entry.name === ui.examples.value);
  ui.examples.value = "";
  if (!example) return;
  openFile(ENTRY);
  const model = modelFor(ENTRY);
  const replaced = model.getValue().trim() && model.getValue() !== loadedExample;
  editor.pushUndoStop();
  editor.executeEdits("example", [{ range: model.getFullModelRange(), text: example.code }]);
  editor.pushUndoStop();
  editor.setPosition({ lineNumber: 1, column: 1 });
  loadedExample = example.code;
  run();
  if (replaced) toast("Ejemplo cargado en main.py. Ctrl+Z recupera tu código.");
});

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
  ui.downloadMenu.hidden = !open;
  ui.downloadToggle.setAttribute("aria-expanded", String(open));
});
document.addEventListener("click", closeMenu);
ui.downloadProject.addEventListener("click", downloadProject);
ui.downloadBundle.addEventListener("click", downloadRecording);

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
    name: typeof saved.name === "string" && saved.name ? saved.name : "animacion",
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
  return name || "animacion";
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
  } else if (event.key === "Escape" && running !== null && !ui.embedDialog.open && ui.contextMenu.hidden && !editing) {
    stop();
  }
});

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
