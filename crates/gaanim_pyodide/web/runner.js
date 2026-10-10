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

// Requirements this Python installed, and those that failed with why: a new
// Python (after Stop) installs them again, from the browser's cache. A
// failed one waits for the page's Reintentar instead of retrying each run.
const installed = new Set();
const failed = new Map();

// One job at a time: an install the page asks for and a run share micropip
// and the project folder.
let queue = Promise.resolve();
function exclusive(job) {
  const done = queue.then(job, job);
  queue = done.catch(() => {});
  return done;
}

// Install the project's requirements with micropip (pure Python wheels from
// PyPI, and the packages Pyodide builds), one by one: one that fails is
// reported and the rest, and the run, go on.
async function installRequirements({ retry = false } = {}) {
  if (retry) failed.clear();
  const read = runner.get("requirements");
  const wanted = read().toJs();
  read.destroy();
  const missing = wanted.filter((requirement) => !installed.has(requirement) && !failed.has(requirement));
  if (missing.length) {
    await pyodide.loadPackage("micropip", { messageCallback: () => {} });
    const micropip = pyodide.pyimport("micropip");
    try {
      for (const requirement of missing) {
        post({ type: "packages", installing: requirement });
        post({ type: "info", text: `Instalando ${requirement}…` });
        try {
          await micropip.install(requirement);
          installed.add(requirement);
        } catch (error) {
          const reason = explain(String(error.message ?? error).trim(), requirement);
          failed.set(requirement, reason);
          post({ type: "stderr", text: `No se pudo instalar ${requirement}. ${reason}` });
        }
      }
    } finally {
      micropip.destroy();
    }
  }
  reportPackages();
}

// The distribution a requirement names, comparable: `Docopt>=0.6` → `docopt`.
const distribution = (requirement) => (/^[A-Za-z0-9._-]+/.exec(requirement.trim())?.[0] ?? "").toLowerCase().replace(/[-_.]+/g, "-");

// Why micropip could not install `requirement` (its whole message), said for
// the page: it may be a dependency that fails, named then.
function explain(message, requirement) {
  const wheel = /pure Python 3 wheel for:?\s*'([^']+)'/.exec(message);
  if (wheel) {
    return distribution(wheel[1]) === distribution(requirement)
      ? "No tiene una rueda de Python puro y Pyodide no lo compila: solo funciona en la aplicación."
      : `Necesita «${wheel[1]}», que no tiene una rueda de Python puro ni la compila Pyodide: solo funciona en la aplicación.`;
  }
  const missing = /Can't fetch metadata for '([^']+)'/.exec(message);
  if (missing) {
    return distribution(missing[1]) === distribution(requirement)
      ? "No está en PyPI: revisa el nombre."
      : `Necesita «${missing[1]}», que no está en PyPI.`;
  }
  if (/Failed to fetch|NetworkError|network/i.test(message)) return "PyPI no responde: comprueba la conexión y reintenta.";
  // micropip ends with a link to its FAQ: the line before says what failed.
  const lines = message.split("\n").map((line) => line.trim()).filter((line) => line && !line.startsWith("See:"));
  return lines.at(-1) ?? message;
}

// The installed distributions and their versions, for the packages panel.
function reportPackages() {
  const found = pyodide.runPython(
    "from importlib.metadata import distributions\n{d.metadata['Name']: d.version for d in distributions() if d.metadata['Name']}",
  );
  const versions = found.toJs({ dict_converter: Object.fromEntries });
  found.destroy();
  post({ type: "packages", versions, failed: Object.fromEntries(failed) });
}

// The page's Añadir: the two files that list packages, as the page has them,
// then their requirements installed without running the scene.
async function install({ files, remove, retry }) {
  writeFiles(files);
  for (const path of remove) {
    try {
      pyodide.FS.unlink(`${PROJECT}/${path}`);
    } catch {
      // It was not there.
    }
  }
  await installRequirements({ retry });
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
    await installRequirements();
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
  if (data.type === "run") exclusive(() => run(data));
  else if (data.type === "install") exclusive(() => install(data));
};
