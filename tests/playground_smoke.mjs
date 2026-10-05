// Smoke test of the web playground's Python runtime, without a browser.
//
// Loads the gaanim wheel that scripts/build_playground.py built into Pyodide
// for Node, runs every example the playground offers plus a scene with an
// uploaded image and music, and checks that each records a playback bundle
// holding them. Exits non-zero on the first failure.
//
// Usage: PYODIDE_DIR=<node_modules/pyodide> node tests/playground_smoke.mjs [dist/playground]
// (`npm install --prefix <dir> pyodide@<version in config.json>`).

import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { pathToFileURL } from "node:url";

const dist = resolve(process.argv[2] ?? "dist/playground");
const config = JSON.parse(readFileSync(join(dist, "config.json"), "utf8"));
const wanted = config.pyodide.match(/\/v([^/]+)\//)?.[1];
const pyodideDir = process.env.PYODIDE_DIR;
if (!pyodideDir) {
  console.error(`set PYODIDE_DIR to an npm install of pyodide@${wanted} (node_modules/pyodide)`);
  process.exit(2);
}
const { loadPyodide } = await import(pathToFileURL(join(pyodideDir, "pyodide.mjs")).href);
const pyodide = await loadPyodide();
if (wanted && pyodide.version !== wanted) {
  console.error(`the playground loads Pyodide ${wanted}, the test has ${pyodide.version}`);
  process.exit(2);
}
let output = [];
pyodide.setStdout({ batched: (text) => output.push(text) });
pyodide.setStderr({ batched: (text) => output.push(text) });
await pyodide.loadPackage(join(dist, config.wheel));
const runner = pyodide.toPy({});
pyodide.runPython(readFileSync(join(dist, "runner.py"), "utf8"), { globals: runner, filename: "runner.py" });

const PROJECT = "/home/pyodide/proyecto";
let failures = 0;

// Run `code` as main.py with `files` next to it. `check(bundle)` names what is
// wrong with the recording; `failsAt` expects the script to fail at that line.
function run(name, code, { files = {}, check = () => null, failsAt = null } = {}) {
  output = [];
  for (const [file, bytes] of Object.entries(files)) pyodide.FS.writeFile(`${PROJECT}/${file}`, bytes);
  const sync = runner.get("sync_files");
  sync(pyodide.toPy(Object.keys(files)));
  pyodide.FS.writeFile(`${PROJECT}/main.py`, code);
  const started = performance.now();
  const value = runner.get("run")(30, true, () => {});
  const result = value.toJs({ dict_converter: Object.fromEntries });
  const seconds = ((performance.now() - started) / 1000).toFixed(2);
  if (failsAt !== null) {
    const error = result.error;
    if (error?.line === failsAt && error.traceback.includes(`File "main.py", line ${failsAt}`)) {
      console.log(`ok   ${name}`);
    } else {
      failures++;
      console.log(`FAIL ${name}: expected an error at main.py line ${failsAt}, got ${JSON.stringify(error ?? result)}`);
    }
    return;
  }
  if (result.error) {
    failures++;
    console.log(`FAIL ${name} (${result.error.phase}): ${result.error.kind}: ${result.error.message}`);
    if (result.error.traceback) console.log(result.error.traceback);
    if (output.length) console.log(output.join("\n"));
    return;
  }
  const bundle = pyodide.FS.readFile("/tmp/escena.gaanim");
  const problem = check(bundle);
  if (problem) {
    failures++;
    console.log(`FAIL ${name}: ${problem}`);
    return;
  }
  console.log(`ok   ${name}: ${bundle.length} bytes in ${seconds} s`);
}

// A bundle is a ZIP: its central directory lists the stored files.
function entries(bundle) {
  const view = new DataView(bundle.buffer, bundle.byteOffset, bundle.byteLength);
  let end = bundle.length - 22;
  while (end >= 0 && view.getUint32(end, true) !== 0x06054b50) end--;
  if (end < 0) return [];
  const names = [];
  let at = view.getUint32(end + 16, true);
  for (let i = view.getUint16(end + 10, true); i > 0; i--) {
    const length = view.getUint16(at + 28, true);
    names.push(new TextDecoder().decode(bundle.subarray(at + 46, at + 46 + length)));
    at += 46 + length + view.getUint16(at + 30, true) + view.getUint16(at + 32, true);
  }
  return names;
}

const examples = JSON.parse(readFileSync(join(dist, "examples.json"), "utf8"));
for (const example of examples) run(example.name, example.code);

// A one-second 440 Hz tone as 16-bit mono WAV.
function wav(seconds = 1, rate = 8000) {
  const samples = seconds * rate;
  const bytes = new Uint8Array(44 + samples * 2);
  const view = new DataView(bytes.buffer);
  const text = (offset, value) => [...value].forEach((char, i) => view.setUint8(offset + i, char.charCodeAt(0)));
  text(0, "RIFF");
  view.setUint32(4, 36 + samples * 2, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, 1, true);
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, samples * 2, true);
  for (let i = 0; i < samples; i++) view.setInt16(44 + i * 2, Math.sin((2 * Math.PI * 440 * i) / rate) * 12000, true);
  return bytes;
}

run(
  "uploaded image and music",
  `from gaanim import BLACK, Scene

scene = Scene(frame=(16, 9), background=BLACK)
photo = scene.media.image("foto.png", width=4)
music = scene.media.audio("tono.wav", end=1.0, fade_out=0.25)
scene.play([music, photo.animate.fade_in().duration(1.0)])
scene.render()
`,
  {
    files: { "foto.png": readFileSync(join(dist, "gaanim-icon-512.png")), "tono.wav": wav() },
    check(bundle) {
      const names = entries(bundle);
      if (!names.some((name) => name.startsWith("media/") && name.endsWith(".wav"))) return `no audio in ${names}`;
      if (!names.includes("tables/images.bin")) return `no image table in ${names}`;
      return null;
    },
  },
);

run("error with its line", "from gaanim import Scene\nscene = Scene()\nscene.missing()\n", { failsAt: 3 });

if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
