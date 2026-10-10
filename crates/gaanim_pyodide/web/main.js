// Gaanim Playground: write a scene in Python, run it in the browser, watch it
// in the web player.
//
// - The editor is Monaco. Completions, signatures, hovers and syntax errors
//   come from Jedi over the gaanim stubs, in a worker (language.js).
// - Scripts run in another worker (runner.js): Pyodide with the gaanim wheel
//   built for it. A run records the scene into a .gaanim bundle.
// - The embed player (`reproductor/insertar/`), the light one a blog gets,
//   plays that bundle; one that needs the GPU passes it leaves out
//   (post-processing, shader backgrounds) plays in the web player (`?embed`).
// - Uploaded files (images, music…) are kept in this browser (IndexedDB) and
//   written next to main.py before each run.
// - Insertar prepares the recording for a blog or website: the file to
//   publish there and the <iframe> of the web player that plays it, with a
//   preview of that player.
// Nothing leaves the browser: the page is static files.

const MONACO = "https://cdn.jsdelivr.net/npm/monaco-editor@0.57.0/min";
const KEYS = {
  code: "gaanim-playground-code",
  fps: "gaanim-playground-fps",
  layout: "gaanim-playground-layout",
  embed: "gaanim-playground-embed",
};

// State the handlers below share. Declared first: editor and worker events can
// fire while the module still awaits its later steps.
let errorLines = [];
let syntaxTimer = 0;
let saveTimer = 0;
let languageReady = false;
// Uploaded files by name. `filesVersion` is bumped on every change; the runner
// receives the files again when it differs from the version it last got.
const files = new Map();
let filesVersion = 0;
let sentVersion = -1;

const $ = (id) => document.getElementById(id);
const ui = {
  run: $("run"),
  stop: $("stop"),
  share: $("share"),
  fps: $("fps"),
  examples: $("examples"),
  downloadToggle: $("download-toggle"),
  downloadMenu: $("download-menu"),
  downloadPy: $("download-py"),
  downloadBundle: $("download-bundle"),
  editor: $("editor"),
  cursor: $("cursor"),
  player: $("player"),
  overlay: $("player-overlay"),
  overlayText: $("player-overlay-text"),
  console: $("console"),
  clearConsole: $("clear-console"),
  files: $("files"),
  fileList: $("file-list"),
  fileInput: $("file-input"),
  drop: $("drop"),
  filesCount: $("files-count"),
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

function setStatus(text, state = "busy") {
  ui.status.textContent = text;
  ui.statusDot.className = `dot ${state}`;
}

let toastTimer = 0;
function toast(text, ms = 2600) {
  ui.toast.textContent = text;
  ui.toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast.hidden = true), ms);
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
// Console
// ---------------------------------------------------------------------------

function consoleEmpty() {
  ui.console.innerHTML = '<p class="empty">La salida de <code>print</code> y los errores aparecen aquí.</p>';
}

function log(text, kind = "") {
  ui.console.querySelector(".empty")?.remove();
  const line = document.createElement("div");
  line.className = `line ${kind}`;
  // `main.py", line N` and `línea N` jump to the editor.
  const pattern = /(main\.py", line (\d+))/g;
  let last = 0;
  for (const match of text.matchAll(pattern)) {
    line.append(text.slice(last, match.index));
    const link = document.createElement("a");
    link.textContent = match[1];
    const number = Number(match[2]);
    link.addEventListener("click", () => revealLine(number));
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

// Tabs of the bottom panel.
for (const tab of document.querySelectorAll(".panel .tab")) {
  tab.addEventListener("click", () => showTab(tab.dataset.tab));
}

function showTab(name) {
  for (const tab of document.querySelectorAll(".panel .tab")) {
    const active = tab.dataset.tab === name;
    tab.classList.toggle("active", active);
    tab.setAttribute("aria-selected", String(active));
  }
  ui.console.hidden = name !== "console";
  ui.files.hidden = name !== "files";
  ui.clearConsole.hidden = name !== "console";
}

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
// Shared links: the code travels compressed in the URL fragment.
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

async function sharedCode() {
  const encoded = new URLSearchParams(location.hash.slice(1)).get("c");
  if (!encoded) return null;
  try {
    return await decompress(encoded);
  } catch {
    toast("El enlace no contiene código válido.");
    return null;
  }
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

const initialCode = (await sharedCode()) ?? load(KEYS.code) ?? examples[0]?.code ?? FALLBACK_CODE;

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
  value: initialCode,
  language: "python",
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
const model = editor.getModel();
// The code face loads after the editor measured its fallback.
document.fonts?.ready.then(() => monaco.editor.remeasureFonts());

function revealLine(line) {
  editor.revealLineInCenterIfOutsideViewport(line);
  editor.setPosition({ lineNumber: line, column: model.getLineFirstNonWhitespaceColumn(line) || 1 });
  editor.focus();
}

editor.onDidChangeCursorPosition(({ position }) => {
  ui.cursor.textContent = `Ln ${position.lineNumber}, Col ${position.column}`;
});

editor.onDidChangeModelContent(() => {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => store(KEYS.code, editor.getValue()), 300);
  // Edited code is no longer the shared one.
  if (location.hash.includes("c=")) history.replaceState(null, "", location.pathname + location.search);
  monaco.editor.setModelMarkers(model, "runtime", []);
  errorLines = editor.deltaDecorations(errorLines, []);
  scheduleSyntaxCheck();
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
  clearTimeout(saveTimer);
  store(KEYS.code, editor.getValue());
  run();
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
    syncLanguageFiles();
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
    const items = await ask("complete", model.getValue(), position.lineNumber, position.column - 1);
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
    const found = await ask("hover", model.getValue(), position.lineNumber, position.column - 1);
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
    const found = await ask("signatures", model.getValue(), position.lineNumber, position.column - 1);
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

function scheduleSyntaxCheck() {
  clearTimeout(syntaxTimer);
  syntaxTimer = setTimeout(async () => {
    const version = model.getVersionId();
    const errors = await ask("errors", model.getValue());
    if (!errors || version !== model.getVersionId()) return;
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

// ---------------------------------------------------------------------------
// Files: images, music… kept in IndexedDB and written next to main.py.
// ---------------------------------------------------------------------------

const database = await new Promise((resolve) => {
  try {
    const request = indexedDB.open("gaanim-playground", 1);
    request.onupgradeneeded = () => request.result.createObjectStore("files", { keyPath: "name" });
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
      const tx = database.transaction("files", mode);
      const result = work(tx.objectStore("files"));
      tx.oncomplete = () => resolve(result?.result ?? null);
      tx.onerror = () => resolve(null);
    } catch {
      resolve(null);
    }
  });
}

for (const record of (await transaction("readonly", (s) => s.getAll())) ?? []) {
  files.set(record.name, record);
}

function fileKind(file) {
  const name = file.name.toLowerCase();
  if (name.endsWith(".svg")) return "svg";
  if (/\.(png|jpe?g|webp|gif|bmp)$/.test(name) || file.type?.startsWith("image/")) return "image";
  if (/\.(mp3|wav|ogg|oga|m4a|aac|flac|opus|weba)$/.test(name) || file.type?.startsWith("audio/")) return "audio";
  return "file";
}

function snippet(file) {
  const name = JSON.stringify(file.name);
  switch (fileKind(file)) {
    case "image":
      return `scene.media.image(${name})`;
    case "svg":
      return `scene.media.svg(${name})`;
    case "audio":
      return `scene.media.audio(${name})`;
    default:
      return name;
  }
}

function safeName(name) {
  // A flat folder: keep the base name, without characters paths reject.
  return name.split(/[\\/]/).pop().replace(/[\u0000-\u001f"]/g, "_").trim() || "archivo";
}

async function addFiles(list) {
  let added = 0;
  for (const file of list) {
    const name = safeName(file.name);
    if (name === "main.py") {
      toast("main.py es el código del editor; renombra el archivo.");
      continue;
    }
    if (file.size > 80 * 1024 * 1024) {
      toast(`«${name}» pesa ${formatSize(file.size)}: el límite es 80 MB.`);
      continue;
    }
    const record = { name, type: file.type, size: file.size, bytes: await file.arrayBuffer(), modified: Date.now() };
    files.set(name, record);
    await transaction("readwrite", (s) => s.put(record));
    added++;
  }
  if (added) {
    filesVersion++;
    renderFiles();
    syncLanguageFiles();
    showTab("files");
    toast(added === 1 ? "Archivo añadido." : `${added} archivos añadidos.`);
  }
}

async function removeFile(name) {
  files.delete(name);
  await transaction("readwrite", (s) => s.delete(name));
  filesVersion++;
  renderFiles();
  syncLanguageFiles();
}

const thumbnails = new Map();
function renderFiles() {
  ui.fileList.textContent = "";
  for (const url of thumbnails.values()) URL.revokeObjectURL(url);
  thumbnails.clear();
  const sorted = [...files.values()].sort((a, b) => a.name.localeCompare(b.name));
  for (const file of sorted) {
    const item = document.createElement("li");
    const thumb = document.createElement("div");
    thumb.className = "thumb";
    const kind = fileKind(file);
    if (kind === "image" || kind === "svg") {
      const url = URL.createObjectURL(new Blob([file.bytes], { type: file.type || (kind === "svg" ? "image/svg+xml" : "") }));
      thumbnails.set(file.name, url);
      thumb.style.backgroundImage = `url("${url}")`;
    } else {
      thumb.textContent = kind === "audio" ? "♪" : (file.name.split(".").pop() || "").slice(0, 4);
    }
    const text = document.createElement("div");
    text.innerHTML = '<div class="name"></div><div class="meta"></div>';
    text.querySelector(".name").textContent = file.name;
    text.querySelector(".name").title = file.name;
    text.querySelector(".meta").textContent = formatSize(file.size);
    const actions = document.createElement("div");
    actions.className = "actions";
    const insert = document.createElement("button");
    insert.type = "button";
    insert.textContent = "Insertar";
    insert.title = `Insertar ${snippet(file)} en el cursor`;
    insert.addEventListener("click", () => {
      const selection = editor.getSelection();
      editor.executeEdits("insert-file", [{ range: selection, text: snippet(file), forceMoveMarkers: true }]);
      editor.focus();
    });
    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "remove";
    remove.textContent = "Quitar";
    remove.addEventListener("click", () => removeFile(file.name));
    actions.append(insert, remove);
    item.append(thumb, text, actions);
    ui.fileList.append(item);
  }
  ui.filesCount.textContent = String(files.size);
  ui.filesCount.hidden = files.size === 0;
}

function syncLanguageFiles() {
  ask("set_files", [...files.keys()]);
}

renderFiles();

ui.fileInput.addEventListener("change", () => {
  addFiles([...ui.fileInput.files]);
  ui.fileInput.value = "";
});
ui.drop.addEventListener("dragover", (event) => {
  event.preventDefault();
  ui.drop.classList.add("over");
});
ui.drop.addEventListener("dragleave", () => ui.drop.classList.remove("over"));

// Files dropped anywhere on the page go to Archivos.
let dragDepth = 0;
const hasFiles = (event) => [...(event.dataTransfer?.types ?? [])].includes("Files");
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
window.addEventListener("drop", (event) => {
  if (!hasFiles(event)) return;
  event.preventDefault();
  dragDepth = 0;
  ui.dropping.hidden = true;
  ui.drop.classList.remove("over");
  addFiles([...event.dataTransfer.files]);
});

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

function startRunner() {
  runnerReady = false;
  sentVersion = -1;
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

function showFailure(error) {
  if (error.traceback) log(error.traceback.trimEnd(), "error");
  else log(`${error.kind}: ${error.message}`, "error");
  showTab("console");
  if (error.line) {
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
    errorLines = editor.deltaDecorations(errorLines, [
      {
        range: new monaco.Range(line, 1, line, 1),
        options: { isWholeLine: true, className: "gaanim-error-line", glyphMarginClassName: "gaanim-error-glyph" },
      },
    ]);
    editor.revealLineInCenterIfOutsideViewport(line);
  }
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
  running = ++runCounter;
  const assetsChanged = sentVersion !== filesVersion;
  const message = {
    type: "run",
    id: running,
    code: editor.getValue(),
    names: [...files.keys()],
    files: assetsChanged ? [...files.values()].map((file) => ({ name: file.name, bytes: file.bytes })) : null,
    fps: Number(ui.fps.value),
    assetsChanged,
  };
  sentVersion = filesVersion;
  monaco.editor.setModelMarkers(model, "runtime", []);
  errorLines = editor.deltaDecorations(errorLines, []);
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

let loadedExample = initialCode;
ui.examples.addEventListener("change", () => {
  const example = examples.find((entry) => entry.name === ui.examples.value);
  ui.examples.value = "";
  if (!example) return;
  const replaced = editor.getValue().trim() && editor.getValue() !== loadedExample;
  editor.pushUndoStop();
  editor.executeEdits("example", [{ range: model.getFullModelRange(), text: example.code }]);
  editor.pushUndoStop();
  editor.setPosition({ lineNumber: 1, column: 1 });
  loadedExample = example.code;
  run();
  if (replaced) toast("Ejemplo cargado. Ctrl+Z recupera tu código.");
});

ui.share.addEventListener("click", async () => {
  const url = `${location.origin}${location.pathname}#c=${await compress(editor.getValue())}`;
  history.replaceState(null, "", url);
  try {
    await navigator.clipboard.writeText(url);
    toast(files.size ? "Enlace copiado. Los archivos subidos no viajan en el enlace." : "Enlace copiado.");
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
ui.downloadPy.addEventListener("click", () => downloadBlob("main.py", editor.getValue(), "text/x-python"));
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
  const saved = (() => {
    try {
      return JSON.parse(load(KEYS.embed) ?? "{}") ?? {};
    } catch {
      return {};
    }
  })();
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
  // The full player opens the published file, or says why it cannot.
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
  } else if (event.key === "Escape" && running !== null && !ui.embedDialog.open) {
    stop();
  }
});

// ---------------------------------------------------------------------------
// Splitters
// ---------------------------------------------------------------------------

const layout = (() => {
  try {
    return JSON.parse(load(KEYS.layout) ?? "{}");
  } catch {
    return {};
  }
})();
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
