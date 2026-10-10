// Video export of the playground: a recording (.gaanim) to MP4, WebM or GIF,
// in the browser.
//
// The embed player's module (`reproductor/insertar/pkg/gaanim_embed.js`),
// the one that plays recordings in the page, draws every frame on a hidden
// canvas at the size asked for. WebCodecs encodes the frames (H.264 for MP4,
// VP9 for WebM) and the scene's audio, which WebAudio mixes as the players
// schedule it; a muxer writes the file. GIF frames are quantized by gifenc.
// Like the embed player, this leaves out GPU passes (post-processing, shader
// backgrounds, transitions and effects): `exportVideo` reports them.

const MP4_MUXER = "https://cdn.jsdelivr.net/npm/mp4-muxer@5.2.2/build/mp4-muxer.mjs";
const WEBM_MUXER = "https://cdn.jsdelivr.net/npm/webm-muxer@5.1.4/build/webm-muxer.mjs";
const GIFENC = "https://cdn.jsdelivr.net/npm/gifenc@1.0.3/dist/gifenc.esm.js";
const SAMPLE_RATE = 48000;

export const FORMATS = {
  mp4: { label: "MP4 (H.264)", extension: "mp4", type: "video/mp4", video: ["avc1.640028", "avc1.4d0028", "avc1.42e01f"], audio: "mp4a.40.2" },
  webm: { label: "WebM (VP9)", extension: "webm", type: "video/webm", video: ["vp09.00.10.08", "vp8"], audio: "opus" },
  gif: { label: "GIF", extension: "gif", type: "image/gif" },
};

let module = null;
// The embed player's module, loaded once from the player folder: its
// bindings and module of one build (`pkg/version.txt`, written by
// scripts/build_web.py), whatever the browser cached.
async function playerModule(playerUrl) {
  if (!module) {
    module = (async () => {
      const build = await fetch(new URL("insertar/pkg/version.txt", playerUrl), { cache: "no-cache" })
        .then((response) => (response.ok ? response.text() : ""))
        .catch(() => "");
      const query = build.trim() ? `?v=${build.trim()}` : "";
      const glue = await import(new URL(`insertar/pkg/gaanim_embed.js${query}`, playerUrl).href);
      const response = await fetch(new URL(`insertar/pkg/gaanim_embed_bg.wasm.gz${query}`, playerUrl));
      let bytes = new Uint8Array(await response.arrayBuffer());
      if (bytes[0] === 0x1f && bytes[1] === 0x8b) {
        const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream("gzip"));
        bytes = new Uint8Array(await new Response(stream).arrayBuffer());
      }
      await glue.default({ module_or_path: bytes });
      return glue;
    })();
  }
  return module;
}

// Whether this browser can encode `format` at `width` by `height`: the
// first video codec it accepts, or null.
export async function supportedCodec(format, width, height, fps) {
  const spec = FORMATS[format];
  if (format === "gif") return "gif";
  if (!("VideoEncoder" in window)) return null;
  for (const codec of spec.video) {
    const config = { codec, width, height, bitrate: bitrateFor(width, height, fps), framerate: fps };
    try {
      if ((await VideoEncoder.isConfigSupported(config)).supported) return codec;
    } catch {
      // Not this one.
    }
  }
  return null;
}

function bitrateFor(width, height, fps) {
  // About 0.1 bit per pixel and frame: sharp vector scenes compress well.
  return Math.round(Math.min(20e6, Math.max(1.5e6, width * height * fps * 0.1)));
}

// Even sizes: 4:2:0 video halves the chroma planes.
export function evenSize(scene, height) {
  const aspect = scene?.width && scene?.height ? scene.width / scene.height : 16 / 9;
  const h = Math.max(2, Math.round(height / 2) * 2);
  return { width: Math.max(2, Math.round((h * aspect) / 2) * 2), height: h };
}

// The scene's audio mixed down to stereo, as the players schedule it: each
// track's start, offset, speed, volume, fades and loop.
async function mixAudio(player, tracks, duration) {
  if (!tracks.length || !(duration > 0)) return null;
  const context = new OfflineAudioContext(2, Math.ceil(duration * SAMPLE_RATE), SAMPLE_RATE);
  const buffers = new Map();
  for (const track of tracks) {
    if (buffers.has(track.media)) continue;
    const bytes = player.media(track.media);
    if (!bytes) continue;
    try {
      buffers.set(track.media, await context.decodeAudioData(bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)));
    } catch {
      // A file this browser cannot decode stays out.
    }
  }
  for (const track of tracks) {
    const buffer = buffers.get(track.media);
    if (!buffer || !(track.speed > 0)) continue;
    const sourceLength = track.sourceDuration ?? Math.max(0, buffer.duration - track.offset);
    const clip = sourceLength / track.speed;
    const active = track.duration != null ? Math.min(track.duration, clip) : clip;
    if (!(clip > 0)) continue;
    const end = track.looping ? duration : Math.min(duration, track.start + active);
    if (end <= track.start) continue;
    const node = context.createBufferSource();
    node.buffer = buffer;
    node.playbackRate.value = track.speed;
    if (track.looping) {
      node.loop = true;
      node.loopStart = track.offset;
      node.loopEnd = track.offset + sourceLength;
    }
    const gain = context.createGain();
    const level = (t) => {
      const elapsed = t - track.start;
      let value = 1;
      if (track.fadeIn > 0) value = Math.min(value, Math.max(0, Math.min(1, elapsed / track.fadeIn)));
      if (track.fadeOut > 0) value = Math.min(value, Math.max(0, Math.min(1, (active - elapsed) / track.fadeOut)));
      return track.volume * value;
    };
    gain.gain.setValueAtTime(level(track.start), track.start);
    if (track.fadeIn > 0) gain.gain.linearRampToValueAtTime(level(track.start + track.fadeIn), track.start + track.fadeIn);
    if (track.fadeOut > 0 && !track.looping) {
      const fadeStart = Math.max(track.start, track.start + active - track.fadeOut);
      gain.gain.setValueAtTime(level(fadeStart), fadeStart);
      gain.gain.linearRampToValueAtTime(0, track.start + active);
    }
    node.connect(gain).connect(context.destination);
    node.start(track.start, track.offset);
    node.stop(end);
  }
  return context.startRendering();
}

function audioConfig(codec) {
  return { codec, sampleRate: SAMPLE_RATE, numberOfChannels: 2, bitrate: 160000 };
}

// Whether this browser encodes audio with `codec`.
async function audioSupported(codec) {
  if (!("AudioEncoder" in window)) return false;
  try {
    return (await AudioEncoder.isConfigSupported(audioConfig(codec))).supported;
  } catch {
    return false;
  }
}

// Encode `audio` (an AudioBuffer) with `codec` into `muxer`.
async function encodeAudio(audio, codec, muxer) {
  const config = audioConfig(codec);
  let failure = null;
  const encoder = new AudioEncoder({
    output: (chunk, meta) => muxer.addAudioChunk(chunk, meta),
    error: (error) => (failure = error),
  });
  encoder.configure(config);
  const left = audio.getChannelData(0);
  const right = audio.getChannelData(1);
  const block = 4096;
  for (let start = 0; start < audio.length; start += block) {
    const frames = Math.min(block, audio.length - start);
    const planar = new Float32Array(frames * 2);
    planar.set(left.subarray(start, start + frames), 0);
    planar.set(right.subarray(start, start + frames), frames);
    const data = new AudioData({
      format: "f32-planar",
      sampleRate: SAMPLE_RATE,
      numberOfFrames: frames,
      numberOfChannels: 2,
      timestamp: Math.round((start / SAMPLE_RATE) * 1e6),
      data: planar,
    });
    encoder.encode(data);
    data.close();
  }
  await encoder.flush();
  encoder.close();
  if (failure) throw failure;
}

// Export `bundle` (the bytes of a .gaanim) as `format` (`mp4`, `webm` or
// `gif`) `height` pixels high. `onProgress(fraction, text)` follows it and
// `signal` cancels it. Resolves to { blob, missing, audio }: `missing` names
// the GPU passes the file uses that the export leaves out.
export async function exportVideo({ bundle, scene, format, height, playerUrl, onProgress = () => {}, signal }) {
  const spec = FORMATS[format];
  onProgress(0, "Preparando el reproductor…");
  const { Player } = await playerModule(playerUrl);
  const size = evenSize(scene, height);
  const canvas = Object.assign(document.createElement("canvas"), { width: size.width, height: size.height });
  canvas.style.cssText = "position: fixed; left: -100000px; top: 0; pointer-events: none;";
  document.body.append(canvas);
  try {
    const player = await Player.create(canvas);
    const info = JSON.parse(player.open(new Uint8Array(bundle)));
    const fps = format === "gif" ? Math.min(info.fps, 25) : info.fps;
    const frames = Math.max(1, Math.ceil(info.duration * fps));
    // `audio` says whether the video has the scene's sound; `sound` whether
    // the scene has any.
    const result = { missing: info.full, audio: false, sound: info.audio.length > 0 };
    const check = () => {
      if (signal?.aborted) throw new DOMException("Exportación cancelada", "AbortError");
    };

    if (format === "gif") {
      const { GIFEncoder, quantize, applyPalette } = await import(GIFENC);
      const gif = GIFEncoder();
      const copy = Object.assign(document.createElement("canvas"), { width: size.width, height: size.height });
      const context = copy.getContext("2d", { willReadFrequently: true });
      for (let index = 0; index < frames; index++) {
        check();
        player.drawFrame(index / fps);
        context.drawImage(canvas, 0, 0);
        const { data } = context.getImageData(0, 0, size.width, size.height);
        const palette = quantize(data, 256);
        gif.writeFrame(applyPalette(data, palette), size.width, size.height, { palette, delay: Math.round(1000 / fps) });
        onProgress((index + 1) / frames, `Fotograma ${index + 1} de ${frames}`);
        // Let the page breathe between frames.
        if (index % 4 === 3) await new Promise((resolve) => setTimeout(resolve, 0));
      }
      gif.finish();
      result.blob = new Blob([gif.bytes()], { type: spec.type });
      return result;
    }

    const codec = await supportedCodec(format, size.width, size.height, fps);
    if (!codec) throw new Error(`Este navegador no puede codificar ${spec.label} a ${size.width} × ${size.height}.`);
    onProgress(0, "Mezclando el audio…");
    // Without an audio encoder for it, the video goes without sound.
    const audio = (await audioSupported(spec.audio)) ? await mixAudio(player, info.audio, info.duration) : null;
    result.audio = Boolean(audio);
    const { Muxer, ArrayBufferTarget } = await import(format === "mp4" ? MP4_MUXER : WEBM_MUXER);
    const audioOptions = audio ? (format === "mp4" ? { codec: "aac", numberOfChannels: 2, sampleRate: SAMPLE_RATE } : { codec: "A_OPUS", numberOfChannels: 2, sampleRate: SAMPLE_RATE }) : undefined;
    const muxer = new Muxer({
      target: new ArrayBufferTarget(),
      video: format === "mp4" ? { codec: "avc", width: size.width, height: size.height, frameRate: fps } : { codec: codec === "vp8" ? "V_VP8" : "V_VP9", width: size.width, height: size.height, frameRate: fps },
      audio: audioOptions,
      ...(format === "mp4" ? { fastStart: "in-memory" } : {}),
      firstTimestampBehavior: "offset",
    });
    let failure = null;
    const encoder = new VideoEncoder({
      output: (chunk, meta) => muxer.addVideoChunk(chunk, meta),
      error: (error) => (failure = error),
    });
    encoder.configure({
      codec,
      width: size.width,
      height: size.height,
      bitrate: bitrateFor(size.width, size.height, fps),
      framerate: fps,
      latencyMode: "quality",
      ...(format === "mp4" ? { avc: { format: "avc" } } : {}),
    });
    const step = 1e6 / fps;
    for (let index = 0; index < frames; index++) {
      check();
      if (failure) throw failure;
      player.drawFrame(index / fps);
      const frame = new VideoFrame(canvas, { timestamp: Math.round(index * step), duration: Math.round(step) });
      encoder.encode(frame, { keyFrame: index % Math.max(1, Math.round(fps * 2)) === 0 });
      frame.close();
      onProgress(((index + 1) / frames) * 0.95, `Fotograma ${index + 1} de ${frames}`);
      while (encoder.encodeQueueSize > 8) await new Promise((resolve) => setTimeout(resolve, 4));
      if (index % 8 === 7) await new Promise((resolve) => setTimeout(resolve, 0));
    }
    await encoder.flush();
    encoder.close();
    if (failure) throw failure;
    if (audio) {
      onProgress(0.97, "Codificando el audio…");
      await encodeAudio(audio, spec.audio, muxer);
    }
    muxer.finalize();
    result.blob = new Blob([muxer.target.buffer], { type: spec.type });
    onProgress(1, "Listo");
    return result;
  } finally {
    canvas.remove();
  }
}
