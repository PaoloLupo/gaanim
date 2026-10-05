// Runner worker: Pyodide with the gaanim wheel built for it.
//
// Each run writes the editor's code and the playground's files into the
// project folder, runs `main.py` and records its scene into a playback
// bundle, whose bytes go back to the page for the web player. A run that
// must stop is stopped by terminating this worker.

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
  const source = await (await fetch(new URL("runner.py", config.base))).text();
  const namespace = pyodide.toPy({});
  pyodide.runPython(source, { globals: namespace, filename: "runner.py" });
  runner = namespace;
  post({ type: "ready", python: pyodide.version });
}

const PROJECT = "/home/pyodide/proyecto";

function writeFiles(files) {
  const { FS } = pyodide;
  for (const file of files) {
    FS.writeFile(`${PROJECT}/${file.name}`, new Uint8Array(file.bytes));
  }
}

async function run({ id, code, files, names, fps, assetsChanged }) {
  const started = performance.now();
  try {
    if (files) writeFiles(files);
    const sync = runner.get("sync_files");
    sync(pyodide.toPy(names));
    sync.destroy();
    pyodide.FS.writeFile(`${PROJECT}/main.py`, code);
    // Packages the script imports (numpy, sympy…) load from Pyodide's index.
    await pyodide.loadPackagesFromImports(code, {
      messageCallback: (text) => post({ type: "info", text }),
      errorCallback: (text) => post({ type: "stderr", text }),
    });
    const runScript = runner.get("run");
    const report = (phase) => post({ type: "phase", id, phase });
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
