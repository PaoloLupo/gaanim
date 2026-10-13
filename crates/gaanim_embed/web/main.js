// Gaanim's embed player: plays a .gaanim file inside another page, such as a
// blog post, at a fraction of the web player's download.
//
//   <iframe src=".../reproductor/insertar/?src=URL&loop&controls=0">
//
// `src` is the file (a parent page on this site may send one instead, see
// below). It plays while at least a quarter of it is on screen and pauses
// when it scrolls away; one paused by hand stays paused. `loop` repeats it,
// `controls=0` hides the bar (a click plays or pauses), `autoplay=0` waits for
// a click and `#t=12.5` starts there. A stop (`scene.stop()`) pauses it until
// a click; one with an ambient loop (`scene.stop(loop=…)`) repeats the loop
// meanwhile, and the click plays on from its end, as a presentation does.
//
// The wasm module (`gaanim_embed`) only decodes the file and draws the frame
// at a time; this page keeps the clock, downloads the file (in pieces when
// its host serves byte ranges, as the web player does), shows the controls and
// plays the audio with WebAudio. A file that needs the GPU passes this player
// leaves out (post-processing, shader backgrounds) opens in the web player
// with the same options instead.
import init, { Player } from "./pkg/gaanim_embed.js";

const params = new URLSearchParams(location.search);
// `?loop`, `?loop=1`: on; `?loop=0`: off; absent: `fallback`.
function flag(name, fallback) {
  if (!params.has(name)) return fallback;
  return !["0", "false", "no"].includes(params.get(name).toLowerCase());
}
const options = {
  loop: flag("loop", false),
  controls: flag("controls", true),
  autoplay: flag("autoplay", true),
};

const $ = (id) => document.getElementById(id);
const canvas = $("canvas");
const message = $("message");
const messageText = $("message-text");
const fillBar = $("fill");
const big = $("big");
const corner = $("corner");
const cornerIcon = $("corner-icon");
const cornerText = $("corner-text");
const bar = $("bar");
const toggle = $("toggle");
const toggleIcon = $("toggle-icon");
const seek = $("seek");
const clock = $("clock");
const loopButton = $("loop");
const mute = $("mute");
const muteWaves = $("mute-waves");
const soundHint = $("sound-hint");

// On a 16-unit grid.
const BAR_PLAY = "M4 2.5v11l9-5.5z";
const BAR_PAUSE = "M4 2.5h3v11H4zM9 2.5h3v11H9z";
const REPLAY = "M8 3.5a4.5 4.5 0 1 1-4.2 2.9l1.3.5A3.1 3.1 0 1 0 8 4.9V7L5 4.2 8 1.5z";

// A parent page on this site (the playground) hears what happens.
function tellParent(kind, text = null, extra = {}) {
  if (window.parent !== window) {
    window.parent.postMessage({ type: "gaanim:status", kind, message: text, ...extra }, location.origin);
  }
}

function say(text, error = false) {
  message.hidden = false;
  message.classList.toggle("error", error);
  message.querySelector(".loader").hidden = error;
  messageText.textContent = text;
}

// ---------------------------------------------------------------------------
// The player and its clock
// ---------------------------------------------------------------------------

let player = null;
// What the module said about the open file: duration, fps, stops, audio…
let info = null;
const state = {
  time: 0,
  playing: false,
  // Started once, by the page or a click.
  started: false,
  ended: false,
  // Resting at this stop until a click.
  stop: null,
  // The stop's ambient loop, `[start, end]`, repeating while it rests there.
  ambient: null,
  // Paused because it scrolled away.
  pausedAway: false,
  // The frame on screen waits for its bytes.
  waiting: false,
  visible: true,
};

function fail(error) {
  const text = error?.message ?? String(error);
  say(text, true);
  tellParent("error", text);
}

function startTime() {
  const value = new URLSearchParams(location.hash.slice(1)).get("t");
  if (!value) return 0;
  const seconds = value.split(":").reduce((total, part) => total * 60 + Number(part), 0);
  return Number.isFinite(seconds) ? Math.max(0, seconds) : 0;
}

// The file the module opened, described by `json`.
function opened(json) {
  info = JSON.parse(json);
  if (info.full.length && fullPlayer) {
    // The web player draws what this one leaves out.
    location.replace(fullPlayer);
    return;
  }
  if (info.background) {
    const [r, g, b] = info.background;
    document.body.style.background = `rgb(${r} ${g} ${b})`;
  }
  document.title = info.title || document.title;
  Object.assign(state, {
    time: Math.min(startTime(), info.duration),
    playing: false,
    started: false,
    ended: false,
    stop: null,
    ambient: null,
    pausedAway: false,
  });
  message.hidden = true;
  bar.hidden = !options.controls;
  seek.max = String(info.duration || 1);
  setAudioTracks(info.audio);
  // `full` names the GPU passes the file uses that this player leaves out.
  tellParent("opened", info.title, { full: info.full });
  followVisibility();
  showControls();
}

function play() {
  if (!info) return;
  // The clock restarts from here, whatever time passed while paused.
  last = null;
  // Out of a stop's loop, from its end.
  if (state.ambient) state.time = state.ambient[1];
  state.ambient = null;
  if (state.ended || state.time >= info.duration) state.time = 0;
  state.started = true;
  state.ended = false;
  state.stop = null;
  state.pausedAway = false;
  state.playing = true;
  showControls();
}

function pause() {
  state.playing = false;
  showControls();
}

// Back to playing where it was: inside a stop's loop, it keeps looping.
function resume() {
  if (!state.ambient) {
    play();
    return;
  }
  last = null;
  state.pausedAway = false;
  state.playing = true;
  showControls();
}

// A click, Space or the bar's button: a stop's loop waits for it to go on.
function togglePlaying() {
  if (state.playing && !state.ambient) pause();
  else play();
}

// Advance the clock by `dt` seconds, resting at stops and at the end.
function advance(dt) {
  const from = state.time;
  let next = from + dt;
  if (state.ambient) {
    // Round the loop, as long as the player rests here.
    const [start, end] = state.ambient;
    state.time = start + ((next - start) % (end - start));
    return;
  }
  const stop = info.stops.find((time) => time > from + 1e-6 && time <= next);
  if (stop !== undefined) {
    state.time = stop;
    state.stop = stop;
    const loop = (info.loops ?? []).find(([start]) => Math.abs(start - stop) < 1e-5);
    if (loop) {
      state.ambient = loop;
      showControls();
    } else {
      pause();
    }
    return;
  }
  if (next >= info.duration) {
    if (options.loop && info.duration > 0) {
      next %= info.duration;
    } else {
      state.time = info.duration;
      state.ended = true;
      pause();
      return;
    }
  }
  state.time = next;
}

// When the clock last advanced; `null` restarts it without a step.
let last = null;
function frame(now) {
  requestAnimationFrame(frame);
  if (!player || !info) {
    last = now;
    return;
  }
  // A stall (a busy page) must not jump far ahead.
  const dt = last === null ? 0 : Math.min((now - last) / 1000, 0.5);
  last = now;
  if (state.playing && !state.waiting) advance(dt);
  try {
    state.waiting = !player.draw(state.time);
  } catch (error) {
    info = null;
    fail(error);
    return;
  }
  requestWanted(state.waiting);
  followAudio();
  showClock();
}

// The canvas has as many pixels as the screen shows.
function fitCanvas(width, height) {
  canvas.width = Math.max(1, Math.round(width));
  canvas.height = Math.max(1, Math.round(height));
}
new ResizeObserver((entries) => {
  const entry = entries[entries.length - 1];
  const box = entry.devicePixelContentBoxSize?.[0];
  if (box) fitCanvas(box.inlineSize, box.blockSize);
  else fitCanvas(entry.contentRect.width * devicePixelRatio, entry.contentRect.height * devicePixelRatio);
}).observe(canvas);

// ---------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------

function formatTime(seconds) {
  const whole = Math.floor(seconds);
  return `${Math.floor(whole / 60)}:${String(whole % 60).padStart(2, "0")}`;
}

let shownClock = "";
function showClock() {
  const text = `${formatTime(state.time)} / ${formatTime(info.duration)}`;
  if (text !== shownClock) {
    clock.textContent = text;
    shownClock = text;
  }
  if (!seeking) seek.value = String(state.time);
  seek.style.setProperty("--progress", `${(100 * state.time) / (info.duration || 1)}%`);
}

let revealTimer = 0;
function showControls() {
  // Before the first start, a button in the middle; at a stop or at the end,
  // a small one in a corner, so the frame the reader looks at stays clear.
  big.hidden = !info || state.started || state.playing;
  const waiting = state.started && ((!state.playing && (state.stop !== null || state.ended)) || state.ambient !== null);
  corner.hidden = !info || !waiting;
  cornerIcon.setAttribute("d", state.ended ? REPLAY : BAR_PLAY);
  cornerText.textContent = state.ended ? "Repetir" : "Continuar";
  const running = state.playing && !state.ambient;
  toggleIcon.setAttribute("d", running ? BAR_PAUSE : state.ended ? REPLAY : BAR_PLAY);
  toggle.setAttribute("aria-label", running ? "Pausar" : state.ended ? "Repetir" : state.ambient ? "Continuar" : "Reproducir");
  loopButton.setAttribute("aria-pressed", String(options.loop));
  document.body.classList.toggle("reveal", hovering);
}

let hovering = false;
function reveal() {
  hovering = true;
  showControls();
  clearTimeout(revealTimer);
  revealTimer = setTimeout(() => {
    hovering = false;
    showControls();
  }, 2500);
}
document.addEventListener("pointermove", reveal);
document.addEventListener("pointerdown", reveal);
document.addEventListener("pointerleave", () => {
  hovering = false;
  showControls();
});

canvas.addEventListener("click", () => {
  if (!info) return;
  canvas.focus();
  togglePlaying();
});
big.addEventListener("click", play);
corner.addEventListener("click", play);
toggle.addEventListener("click", togglePlaying);
loopButton.addEventListener("click", () => {
  options.loop = !options.loop;
  // Looping a finished animation plays it again.
  if (options.loop && state.ended) play();
  showControls();
});
canvas.addEventListener("keydown", (event) => {
  if (event.key === " " || event.key === "k") {
    event.preventDefault();
    togglePlaying();
  }
});

let seeking = false;
seek.addEventListener("input", () => {
  seeking = true;
  state.time = Number(seek.value);
  state.ended = false;
  state.stop = null;
  state.ambient = null;
});
seek.addEventListener("change", () => {
  seeking = false;
});

$("fullscreen").addEventListener("click", () => {
  const root = document.documentElement;
  if (document.fullscreenElement) document.exitFullscreen();
  else (root.requestFullscreen ?? root.webkitRequestFullscreen)?.call(root);
});

// ---------------------------------------------------------------------------
// On screen
// ---------------------------------------------------------------------------

// A hidden tab draws nothing: on its return the clock goes on from there.
document.addEventListener("visibilitychange", () => {
  last = null;
});

function followVisibility() {
  if (!info) return;
  if (state.visible && options.autoplay && (!state.started || state.pausedAway)) {
    resume();
  } else if (!state.visible && state.playing) {
    pause();
    state.pausedAway = true;
  }
}

if ("IntersectionObserver" in window) {
  state.visible = false;
  // A frame's document intersects the viewport of the page around it.
  new IntersectionObserver(
    (entries) => {
      const entry = entries[entries.length - 1];
      state.visible = entry.isIntersecting && entry.intersectionRatio >= 0.25;
      followVisibility();
    },
    { threshold: [0, 0.25, 0.5] },
  ).observe(document.documentElement);
}

// ---------------------------------------------------------------------------
// Downloading
// ---------------------------------------------------------------------------

// Files at least this large download in pieces when the host allows it.
const PIECES_FROM = 2 << 20;
// The end of the file fetched first: the archive's directory and the tables.
const TAIL = 1 << 20;
// Size of each background request.
const FILL_BLOCK = 4 << 20;
// Requests closer than this are joined into one.
const JOIN_GAP = 64 << 10;

// The file downloading in pieces: { url, length, have, inflight, retryAt }.
let remote = null;

function httpError(response) {
  return Object.assign(new Error(`HTTP ${response.status}`), { status: response.status });
}

function downloadError(error) {
  if (error.status === 404 || error.status === 410) return "No se encontró la animación: revisa la dirección del archivo.";
  if (error.status) return `El sitio donde está la animación respondió con un error (HTTP ${error.status}).`;
  if (!navigator.onLine) return "No hay conexión a internet.";
  return "El sitio donde está la animación no permite abrirla desde otra página (CORS), o no hay conexión.";
}

async function download(url, onProgress) {
  const response = await fetch(url);
  if (!response.ok) throw httpError(response);
  const total = Number(response.headers.get("Content-Length")) || 0;
  const reader = response.body.getReader();
  const chunks = [];
  let received = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    received += value.length;
    if (total) onProgress(received / total);
  }
  const bytes = new Uint8Array(received);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  return bytes;
}

async function fetchRange(url, start, end) {
  const response = await fetch(url, { headers: { Range: `bytes=${start}-${end - 1}` } });
  if (response.status !== 206) throw response.ok ? new RangeError("byte ranges not supported") : httpError(response);
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length !== end - start) throw new RangeError("byte ranges not supported");
  return bytes;
}

function addRange(ranges, start, end) {
  ranges.push([start, end]);
  ranges.sort((a, b) => a[0] - b[0]);
  const merged = [];
  for (const range of ranges) {
    const previous = merged[merged.length - 1];
    if (previous && range[0] <= previous[1]) previous[1] = Math.max(previous[1], range[1]);
    else merged.push([...range]);
  }
  ranges.splice(0, ranges.length, ...merged);
}

// The parts of [start, end) that none of `lists` covers.
function gaps(start, end, ...lists) {
  const covered = lists.flat().sort((a, b) => a[0] - b[0]);
  const result = [];
  let at = start;
  for (const [from, to] of covered) {
    if (to <= at) continue;
    if (from >= end) break;
    if (from > at) result.push([at, Math.min(from, end)]);
    at = Math.max(at, to);
    if (at >= end) break;
  }
  if (at < end) result.push([at, end]);
  return result;
}

function showFill() {
  const have = remote.have.reduce((sum, [start, end]) => sum + end - start, 0);
  fillBar.hidden = have >= remote.length;
  fillBar.style.width = `${(100 * have) / remote.length}%`;
}

async function request(r, start, end) {
  const entry = [start, end];
  r.inflight.push(entry);
  try {
    const bytes = await fetchRange(r.url, start, end);
    if (remote !== r) return;
    addRange(r.have, start, end);
    showFill();
    const json = player.addBytes(start, bytes);
    if (json) opened(json);
    requestWanted(false);
  } catch (error) {
    if (remote !== r) return;
    if (error.name === "damaged" || error instanceof RangeError) {
      // A host that compresses on the fly breaks byte ranges: read it whole.
      remote = null;
      playWhole(r.url);
      return;
    }
    r.retryAt = Date.now() + 2000;
  } finally {
    r.inflight.splice(r.inflight.indexOf(entry), 1);
  }
}

// Download the byte ranges the module asked for.
function requestWanted() {
  if (!player) return;
  const flat = player.takeWanted();
  const r = remote;
  if (!r || !flat.length || Date.now() < r.retryAt) return;
  const wanted = [];
  for (let index = 0; index + 1 < flat.length; index += 2) wanted.push([flat[index], flat[index + 1]]);
  wanted.sort((a, b) => a[0] - b[0]);
  const joined = [];
  for (const range of wanted) {
    const previous = joined[joined.length - 1];
    if (previous && range[0] <= previous[1] + JOIN_GAP) previous[1] = Math.max(previous[1], range[1]);
    else joined.push([...range]);
  }
  for (const [start, end] of joined) {
    for (const [from, to] of gaps(start, end, r.have, r.inflight)) request(r, from, to);
  }
}

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// Download the rest of the file in the background, in order.
async function fill(r) {
  while (remote === r && gaps(0, r.length, r.have).length) {
    const wait = r.retryAt - Date.now();
    if (wait > 0) {
      await sleep(wait);
      continue;
    }
    const gap = gaps(0, r.length, r.have, r.inflight)[0];
    if (!gap) {
      await sleep(200);
      continue;
    }
    await request(r, gap[0], Math.min(gap[1], gap[0] + FILL_BLOCK));
  }
}

async function playWhole(url) {
  say("Cargando la animación…");
  let bytes;
  try {
    bytes = await download(url, (fraction) => {
      fillBar.hidden = false;
      fillBar.style.width = `${100 * fraction}%`;
    });
  } catch (error) {
    fail(downloadError(error));
    return;
  }
  fillBar.hidden = true;
  openBytes(bytes);
}

function openBytes(bytes) {
  remote = null;
  try {
    opened(player.open(bytes));
  } catch (error) {
    fail(error);
  }
}

async function playUrl(url) {
  url = new URL(url, location.href).href;
  say("Cargando la animación…");
  let length = 0;
  try {
    const head = await fetch(url, { method: "HEAD" });
    if (head.status === 404 || head.status === 410) throw httpError(head);
    if (head.ok) length = Number(head.headers.get("Content-Length")) || 0;
  } catch (error) {
    fail(downloadError(error));
    return;
  }
  if (length >= PIECES_FROM) {
    const start = Math.max(0, length - TAIL);
    let tail = null;
    try {
      tail = await fetchRange(url, start, length);
    } catch {
      // No byte ranges (or not across origins): download it whole.
    }
    // A .gaanim ends with the ZIP end record; a host that compresses on the
    // fly reports the compressed length, so this tail would not be the end.
    const end = tail && tail.length - 22;
    if (tail && end >= 0 && tail[end] === 0x50 && tail[end + 1] === 0x4b && tail[end + 2] === 5 && tail[end + 3] === 6) {
      const r = { url, length, have: [[start, length]], inflight: [], retryAt: 0 };
      remote = r;
      showFill();
      try {
        const json = player.openRemote(length, start, tail);
        if (json) opened(json);
        requestWanted();
      } catch (error) {
        remote = null;
        if (error.name === "damaged") playWhole(url);
        else fail(error);
        return;
      }
      fill(r);
      return;
    }
  }
  await playWhole(url);
}

// ---------------------------------------------------------------------------
// Audio, scheduled with WebAudio against the clock (as the web player does)
// ---------------------------------------------------------------------------

const audio = { ctx: null, master: null, tracks: [], buffers: new Map(), sources: new Map(), anchor: null, generation: 0, muted: false };
// Sources start this far ahead of the playhead, in timeline seconds.
const AUDIO_HORIZON = 10;
// A playhead further than this from the audio clock reschedules every source.
const AUDIO_TOLERANCE = 0.12;

function audioContext() {
  if (!audio.ctx) {
    const Context = window.AudioContext ?? window.webkitAudioContext;
    if (!Context) return null;
    audio.ctx = new Context();
    audio.ctx.onstatechange = showSoundHint;
    audio.master = audio.ctx.createGain();
    audio.master.connect(audio.ctx.destination);
  }
  return audio.ctx;
}

// A browser starts audio only after a click or key on the page.
function showSoundHint() {
  soundHint.hidden = !(audio.tracks.length && audio.ctx && audio.ctx.state !== "running" && !audio.muted);
}
for (const type of ["pointerdown", "keydown", "touchend"]) {
  window.addEventListener(type, () => audio.ctx?.state === "suspended" && audio.ctx.resume(), true);
}

function stopAudio() {
  for (const { node, gain } of audio.sources.values()) {
    try {
      node.stop();
    } catch {
      // Not started yet, or already stopped.
    }
    node.disconnect();
    gain.disconnect();
  }
  audio.sources.clear();
  audio.anchor = null;
}

function setAudioTracks(tracks) {
  stopAudio();
  audio.generation += 1;
  audio.tracks = tracks;
  audio.buffers.clear();
  audio.pending = [...new Set(tracks.map((track) => track.media))];
  mute.hidden = !tracks.length;
  if (tracks.length) audioContext();
  showSoundHint();
}

// Hand the decoder each audio file once its bytes are here.
function loadAudio() {
  if (!audio.pending?.length || !audio.ctx) return;
  const generation = audio.generation;
  audio.pending = audio.pending.filter((entry) => {
    let bytes;
    try {
      bytes = player.media(entry);
    } catch {
      return false;
    }
    if (!bytes) return true;
    audio.ctx.decodeAudioData(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)).then(
      (buffer) => audio.generation === generation && audio.buffers.set(entry, buffer),
      () => {},
    );
    return false;
  });
}

function trackLevel(track, active, t) {
  const elapsed = t - track.start;
  let level = 1;
  if (track.fadeIn > 0) level = Math.min(level, Math.max(0, Math.min(1, elapsed / track.fadeIn)));
  if (track.fadeOut > 0) level = Math.min(level, Math.max(0, Math.min(1, (active - elapsed) / track.fadeOut)));
  return track.volume * level;
}

function scheduleAudio(time, now) {
  const ctx = audio.ctx;
  const at = (t) => now + (t - time);
  audio.tracks.forEach((track, index) => {
    if (audio.sources.has(index) || !(track.speed > 0)) return;
    const buffer = audio.buffers.get(track.media);
    if (!buffer) return;
    const sourceLength = track.sourceDuration ?? Math.max(0, buffer.duration - track.offset);
    const clip = sourceLength / track.speed;
    const active = track.duration != null ? Math.min(track.duration, clip) : clip;
    if (!(clip > 0)) return;
    const end = track.looping ? Infinity : track.start + active;
    if (end <= time + 1e-3 || track.start > time + AUDIO_HORIZON) return;
    const from = Math.max(time, track.start);
    const elapsed = from - track.start;
    const node = ctx.createBufferSource();
    node.buffer = buffer;
    node.playbackRate.value = track.speed;
    if (track.looping) {
      node.loop = true;
      node.loopStart = track.offset;
      node.loopEnd = track.offset + sourceLength;
    }
    const gain = ctx.createGain();
    const level = gain.gain;
    level.setValueAtTime(trackLevel(track, active, from), at(from));
    const fadeInEnd = track.start + track.fadeIn;
    if (track.fadeIn > 0 && fadeInEnd > from) level.linearRampToValueAtTime(trackLevel(track, active, fadeInEnd), at(fadeInEnd));
    if (track.fadeOut > 0) {
      const fadeOutStart = Math.max(from, track.start + active - track.fadeOut, fadeInEnd);
      level.setValueAtTime(trackLevel(track, active, fadeOutStart), at(fadeOutStart));
      level.linearRampToValueAtTime(0, at(track.start + active));
    }
    node.connect(gain).connect(audio.master);
    node.start(at(from), track.offset + (track.looping ? elapsed % clip : elapsed) * track.speed);
    if (Number.isFinite(end)) node.stop(at(end));
    node.onended = () => {
      if (audio.sources.get(index)?.node === node) {
        audio.sources.delete(index);
        gain.disconnect();
      }
    };
    audio.sources.set(index, { node, gain });
  });
}

// Follow the clock every frame.
function followAudio() {
  if (!audio.tracks.length) return;
  loadAudio();
  const ctx = audio.ctx;
  if (!ctx || !state.playing || state.waiting || ctx.state !== "running" || document.hidden || audio.muted) {
    if (audio.anchor) stopAudio();
    return;
  }
  const now = ctx.currentTime;
  const anchor = audio.anchor;
  const expected = anchor && anchor.time + (now - anchor.at);
  if (!anchor || Math.abs(expected - state.time) > AUDIO_TOLERANCE) {
    stopAudio();
    audio.anchor = { time: state.time, at: now };
  }
  scheduleAudio(audio.anchor.time + (now - audio.anchor.at), now);
}

mute.addEventListener("click", () => {
  audio.muted = !audio.muted;
  muteWaves.style.display = audio.muted ? "none" : "";
  mute.setAttribute("aria-label", audio.muted ? "Activar el sonido" : "Silenciar");
  showSoundHint();
});

// ---------------------------------------------------------------------------
// Start
// ---------------------------------------------------------------------------

const src = params.get("src");
// The web player, with the same file and options, for a file that needs it.
const fullPlayer = src
  ? (() => {
      const full = new URL("../", location.href);
      full.search = location.search;
      // The web player lives a folder up: hand it the file's address as
      // resolved here, or a relative one would point past the site's base.
      full.searchParams.set("src", new URL(src, location.href).href);
      full.searchParams.set("embed", "");
      full.hash = location.hash;
      return full.href;
    })()
  : null;

// The module ships gzip-compressed so it downloads small from any host; a
// server that already decoded it hands over the plain module.
async function gunzip(bytes) {
  if (bytes[0] !== 0x1f || bytes[1] !== 0x8b) return bytes;
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

async function start() {
  if (!navigator.gpu) {
    say("Este navegador no tiene WebGPU, que la animación necesita. Usa una versión reciente de Chrome, Edge, Safari o Firefox.", true);
    tellParent("error", "WebGPU");
    return;
  }
  try {
    const module = await gunzip(new Uint8Array(await (await fetch("pkg/gaanim_embed_bg.wasm.gz")).arrayBuffer()));
    await init({ module_or_path: module });
    player = await Player.create(canvas);
  } catch (error) {
    fail(error);
    return;
  }
  requestAnimationFrame(frame);
  tellParent("ready");
  if (src) playUrl(src);
  else if (window.parent === window) say("Falta la dirección de la animación (?src=).", true);
}

// A parent page on this site may send a file and change how it plays.
window.addEventListener("message", (event) => {
  if (event.origin !== location.origin || event.source !== window.parent) return;
  const data = event.data;
  if (data?.type === "gaanim:open" && data.bytes && player) {
    openBytes(new Uint8Array(data.bytes));
  } else if (data?.type === "gaanim:options") {
    for (const key of ["loop", "controls", "autoplay"]) {
      if (typeof data[key] === "boolean") options[key] = data[key];
    }
    bar.hidden = !info || !options.controls;
    followVisibility();
    showControls();
  }
});

tellParent("loaded");
start();
