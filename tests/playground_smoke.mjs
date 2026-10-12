// Smoke test of the web playground's Python runtime, without a browser.
//
// Loads the gaanim wheel that scripts/build_playground.py built into Pyodide
// for Node, runs every example the playground offers, a scene with an uploaded
// image and music, and projects with folders (a module that changes between
// runs, a font, an error inside a module), and checks that each records a
// playback bundle holding what it should. Exits non-zero on any failure.
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

// Run `code` as main.py with `files` (by path, folders included) next to it,
// as the page does. `check(bundle, output)` names what is wrong with the
// recording or what the script printed; `failsAt` expects the run to fail at
// that line of `failsIn`.
function run(name, code, { files = {}, check = () => null, failsAt = null, failsIn = "main.py" } = {}) {
  output = [];
  const sync = runner.get("sync_files");
  sync(pyodide.toPy(Object.keys(files)), pyodide.toPy([]));
  for (const [file, bytes] of Object.entries(files)) {
    const path = `${PROJECT}/${file}`;
    pyodide.FS.mkdirTree(path.slice(0, path.lastIndexOf("/")));
    pyodide.FS.writeFile(path, bytes);
  }
  pyodide.FS.writeFile(`${PROJECT}/main.py`, code);
  const started = performance.now();
  // The phases the page hears: the recording's, with how far it went.
  const recorded = [];
  const report = (phase, done = 0, total = 0) => phase === "record" && recorded.push([done, total]);
  const value = runner.get("run")(30, true, report);
  const result = value.toJs({ dict_converter: Object.fromEntries });
  const seconds = ((performance.now() - started) / 1000).toFixed(2);
  if (failsAt !== null) {
    const error = result.error;
    if (error?.file === failsIn && error?.line === failsAt && error.traceback.includes(`File "${failsIn}", line ${failsAt}`)) {
      console.log(`ok   ${name}`);
    } else {
      failures++;
      console.log(`FAIL ${name}: expected an error at ${failsIn} line ${failsAt}, got ${JSON.stringify(error ?? result)}`);
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
  // The page sizes the <iframe> that embeds the recording with these.
  if (!(result.width > 0 && result.height > 0 && result.duration >= 0)) {
    failures++;
    console.log(`FAIL ${name}: no size or duration in ${JSON.stringify(result)}`);
    return;
  }
  // The page shows the recording's progress: it must reach its end.
  const [done, total] = recorded.at(-1) ?? [0, 0];
  if (!(total > 0 && done === total)) {
    failures++;
    console.log(`FAIL ${name}: the recording reported ${JSON.stringify(recorded)}`);
    return;
  }
  const bundle = pyodide.FS.readFile("/tmp/escena.gaanim");
  const problem = check(bundle, output.join("\n"));
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

// A project: main.py imports a module from a folder, which changes between
// runs; the second run must see the change.
const encoder = new TextEncoder();
const MODULE_MAIN = `from gaanim import BLACK, Scene
from lib.formas import radio

print("radio", radio())
scene = Scene(frame=(16, 9), background=BLACK)
circle = scene.geometry.circle(radio())
scene.play([circle.animate.create().duration(0.5)])
scene.render()
`;
for (const value of [1.5, 2.5]) {
  run(`module in a folder (radio ${value})`, MODULE_MAIN, {
    files: { "lib/formas.py": encoder.encode(`def radio():\n    return ${value}\n`) },
    check: (_, printed) => (printed.includes(`radio ${value}`) ? null : `printed ${JSON.stringify(printed)}`),
  });
}

run(
  "error inside a module",
  "from lib.roto import falla\nfalla()\n",
  { files: { "lib/roto.py": encoder.encode("def falla():\n    x = 1\n    raise ValueError('roto')\n") }, failsAt: 3, failsIn: "lib/roto.py" },
);

// A font uploaded to a folder of the project, embedded through the theme.
run(
  "uploaded font",
  `from gaanim import Scene, Theme

scene = Scene(frame=(16, 9), theme=Theme("technical", font_dir="assets/fonts", fonts={"text": "Victor Mono"}))
title = scene.text("Fuente propia")
scene.play([title.animate.write().duration(0.5)])
scene.render()
`,
  { files: { "assets/fonts/VictorMono.ttf": readFileSync(resolve("docs/fixtures/assets/fonts/VictorMono-VariableFont.ttf")) } },
);

// The packages a project asks for: its pyproject.toml (gaanim left out) and
// its requirements.txt (comments and pip options left out). Reading only:
// installing needs PyPI.
{
  const sync = runner.get("sync_files");
  sync(pyodide.toPy(["pyproject.toml", "requirements.txt"]), pyodide.toPy([]));
  pyodide.FS.writeFile(`${PROJECT}/pyproject.toml`, '[project]\nname = "x"\ndependencies = ["gaanim", "rich>=13"]\n');
  pyodide.FS.writeFile(`${PROJECT}/requirements.txt`, "# para el gráfico\nmakefun\n-r otros.txt\n\nrich>=13\n");
  const read = runner.get("requirements");
  const wanted = read().toJs();
  const expected = ["rich>=13", "makefun"];
  if (JSON.stringify(wanted) === JSON.stringify(expected)) {
    console.log("ok   requirements");
  } else {
    failures++;
    console.log(`FAIL requirements: expected ${JSON.stringify(expected)}, got ${JSON.stringify(wanted)}`);
  }
  sync(pyodide.toPy([]), pyodide.toPy([]));
}

if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
