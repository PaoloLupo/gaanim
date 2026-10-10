// Runner worker: Pyodide with the gaanim wheel built for it.
//
// Each run writes the playground's project (`main.py`, its modules and its
// assets, in folders) into the project folder, runs `main.py` and records its
// scene into a playback bundle, whose bytes go back to the page for the
// player. A run that must stop is stopped by terminating this worker.

let pyodide = null;
let runner = null;

function post(message, transfer = []) {
  postMessage(message, transfer);
}

function progress(text) {
  post({ type: "progress", text });
}

async function init(config) {
  progress("Descargando Python…");
  const { loadPyodide } = await import(`${config.pyodide}pyodide.mjs`);
  pyodide = await loadPyodide({ indexURL: config.pyodide });
  pyodide.setStdout({ batched: (text) => post({ type: "stdout", text }) });
  pyodide.setStderr({ batched: (text) => post({ type: "stderr", text }) });
  progress("Descargando Gaanim…");
  await pyodide.loadPackage(new URL(config.wheel, config.base).href, {
    messageCallback: () => {},
    errorCallback: (text) => post({ type: "stderr", text }),
  });
  progress("Iniciando Gaanim…");
  // Revalidated: a cached runner.py may be older than this runner.js.
  const source = await (await fetch(new URL("runner.py", config.base), { cache: "no-cache" })).text();
  const namespace = pyodide.toPy({});
  pyodide.runPython(source, { globals: namespace, filename: "runner.py" });
  runner = namespace;
  post({ type: "ready", python: pyodide.runPython("import sys; '.'.join(map(str, sys.version_info[:3]))"), pyodide: pyodide.version });
}

const PROJECT = "/home/pyodide/proyecto";

// Requirements this Python installed: a new one (after Stop) installs them
// again, from the browser's cache.
const installed = new Set();

// Install the project's requirements from PyPI with micropip: pure Python
// wheels, and the packages Pyodide builds for the browser.
async function installRequirements() {
  const read = runner.get("requirements");
  const wanted = read().toJs();
  read.destroy();
  const missing = wanted.filter((requirement) => !installed.has(requirement));
  if (!missing.length) return;
  post({ type: "info", text: `Instalando ${missing.join(", ")}…` });
  await pyodide.loadPackage("micropip", { messageCallback: () => {} });
  const micropip = pyodide.pyimport("micropip");
  try {
    await micropip.install(pyodide.toPy(missing));
  } catch (error) {
    const reason = String(error.message ?? error).trim().split("\n").at(-1);
    throw new Error(
      `No se pudo instalar ${missing.join(", ")}: ${reason}\nEn el navegador se instalan los paquetes de Python puro de PyPI y los que Pyodide compila; los que traen código nativo propio solo funcionan en la aplicación.`,
    );
  } finally {
    micropip.destroy();
  }
  for (const requirement of missing) installed.add(requirement);
  post({ type: "info", text: `Instalado: ${missing.join(", ")}.` });
}

// Write `files` ({ path, text } or { path, bytes }) into the project folder.
function writeFiles(files) {
  const { FS } = pyodide;
  for (const file of files) {
    const path = `${PROJECT}/${file.path}`;
    FS.mkdirTree(path.slice(0, path.lastIndexOf("/")));
    FS.writeFile(path, file.text ?? new Uint8Array(file.bytes));
  }
}

async function run({ id, files, paths, folders, imports, fps, assetsChanged }) {
  const started = performance.now();
  try {
    // The project's files and folders as the page has them: removed ones
    // go, changed ones are written again.
    const sync = runner.get("sync_files");
    sync(pyodide.toPy(paths), pyodide.toPy(folders));
    sync.destroy();
    writeFiles(files);
    try {
      await installRequirements();
    } catch (error) {
      post({ type: "failed", id, error: { phase: "install", kind: "Paquetes", message: error.message, traceback: "", line: null } });
      return;
    }
    // Packages the project imports (numpy, sympy…) load from Pyodide's index.
    await pyodide.loadPackagesFromImports(imports, {
      messageCallback: (text) => post({ type: "info", text }),
      errorCallback: (text) => post({ type: "stderr", text }),
    });
    const runScript = runner.get("run");
    // The recording says how far it went; the page shows it.
    const report = (phase, done = 0, total = 0) => post({ type: "phase", id, phase, done, total });
    let result;
    try {
      const value = runScript(fps, assetsChanged, report);
      result = value.toJs({ dict_converter: Object.fromEntries });
      value.destroy();
    } finally {
      runScript.destroy();
    }
    if (result.error) {
      post({ type: "failed", id, error: result.error });
      return;
    }
    const bytes = pyodide.FS.readFile("/tmp/escena.gaanim");
    post(
      {
        type: "done",
        id,
        bytes: bytes.buffer,
        timings: { run: result.run, record: result.record, total: (performance.now() - started) / 1000 },
        scene: { width: result.width, height: result.height, duration: result.duration },
      },
      [bytes.buffer],
    );
  } catch (error) {
    post({
      type: "failed",
      id,
      error: { phase: "internal", kind: "Error", message: String(error), traceback: "", line: null },
    });
  }
}

let ready = null;
self.onmessage = async ({ data }) => {
  if (data.type === "init") {
    ready = init(data.config).catch((error) => post({ type: "fatal", error: String(error) }));
    return;
  }
  await ready;
  if (!runner) return;
  if (data.type === "run") run(data);
};
