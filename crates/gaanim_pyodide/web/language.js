// Editor services worker: Jedi in its own Pyodide, over the gaanim stubs.
//
// It never loads the engine, so completions work while the runner downloads
// or records a scene. Requests that a newer one of the same kind superseded
// while Python was busy are answered with null instead of computed.

let services = null;
let pyodide = null;
const pending = new Map();
let draining = false;

function reply(id, result, error = null) {
  postMessage({ id, result, error });
}

async function init(config) {
  const { loadPyodide } = await import(`${config.pyodide}pyodide.mjs`);
  pyodide = await loadPyodide({ indexURL: config.pyodide });
  await pyodide.loadPackage("jedi");
  const stubs = await fetch(new URL(config.stubs, config.base));
  if (!stubs.ok) throw new Error(`stubs.zip: HTTP ${stubs.status}`);
  pyodide.unpackArchive(await stubs.arrayBuffer(), "zip", { extractDir: "/home/pyodide/stubs" });
  const source = await (await fetch(new URL("language.py", config.base))).text();
  const namespace = pyodide.toPy({});
  pyodide.runPython(source, { globals: namespace, filename: "language.py" });
  services = namespace;
  // Parse the stubs once, so the first real completion is quick.
  call("complete", ["from gaanim import Scene\nscene = Scene()\nscene.", 3, 6]);
}

function toJs(value) {
  if (value && typeof value.toJs === "function") {
    const converted = value.toJs({ dict_converter: Object.fromEntries });
    value.destroy();
    return converted;
  }
  return value;
}

function call(method, args) {
  const fn = services.get(method);
  try {
    return toJs(fn(...args.map((arg) => (Array.isArray(arg) ? pyodide.toPy(arg) : arg))));
  } finally {
    fn.destroy();
  }
}

async function drain() {
  if (draining) return;
  draining = true;
  // Let queued messages arrive before choosing what to compute.
  await new Promise((resolve) => setTimeout(resolve, 0));
  while (pending.size) {
    const [method, { id, args }] = pending.entries().next().value;
    pending.delete(method);
    try {
      reply(id, call(method, args));
    } catch (error) {
      reply(id, null, String(error));
    }
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  draining = false;
}

let ready = null;
self.onmessage = async ({ data }) => {
  if (data.type === "init") {
    ready = init(data.config).then(
      () => postMessage({ type: "ready" }),
      (error) => postMessage({ type: "failed", error: String(error) }),
    );
    return;
  }
  await ready;
  if (!services) return reply(data.id, null, "not ready");
  // `set_files` and `resolve` are cheap and must not be skipped.
  if (data.method === "set_files" || data.method === "resolve") {
    try {
      reply(data.id, call(data.method, data.args));
    } catch (error) {
      reply(data.id, null, String(error));
    }
    return;
  }
  const superseded = pending.get(data.method);
  if (superseded) reply(superseded.id, null);
  pending.set(data.method, { id: data.id, args: data.args });
  drain();
};
