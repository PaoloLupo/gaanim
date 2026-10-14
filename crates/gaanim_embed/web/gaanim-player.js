// <gaanim-player>: a .gaanim file in any page, played by the embed player
// that sits next to this script.
//
//   <script type="module" src="https://paololupo.github.io/gaanim/reproductor/insertar/gaanim-player.js"></script>
//   <gaanim-player src="animaciones/circulo.gaanim" loop controls="0" label="Un círculo"></gaanim-player>
//
// The element is as wide as its container and takes the file's proportion
// once it opens (16:9 until then; `ratio="1 / 1"` sets one from the start,
// and a page's own `aspect-ratio` wins over both). Its frame loads lazily.
//
// Attributes: `src` (relative to the page), `loop`, `controls`, `autoplay`,
// `preload` and `full`, passed to the player as its options are
// (`controls="0"`, `full="click"`…), `start` (seconds), `label` (what a
// screen reader announces) and `ratio`.
//
// Methods `play()`, `pause()` and `seek(seconds)`; properties `currentTime`
// (which seeks when set), `duration`, `paused`, `ended`, `loop` and `muted`.
// They act once the file is open (the `opened` event).
//
// Events, each with the player's message as `detail`: `ready`, `opened`
// (with `width`, `height`, `fps`, `stops`), `redirect` (the file needs the
// full web player, which takes over the frame), `full` (the same with
// `full="click"`), `play`, `pause`, `timeupdate`, `stop`, `ended` and
// `error`.

const PLAYER = new URL("./", import.meta.url);
// Attributes the player reads as its own options.
const OPTIONS = ["loop", "controls", "autoplay", "preload", "full"];

class GaanimPlayer extends HTMLElement {
  static observedAttributes = ["src", "start", "label", "ratio", ...OPTIONS];

  #frame = null;
  #state = { time: 0, duration: NaN, playing: false, ended: false, muted: false };
  #onMessage = (event) => this.#heard(event);

  connectedCallback() {
    if (!this.#frame) {
      const root = this.attachShadow({ mode: "open" });
      const style = document.createElement("style");
      style.textContent =
        ":host { display: block; aspect-ratio: var(--gaanim-ratio, 16 / 9); }" +
        " iframe { display: block; width: 100%; height: 100%; border: 0; }";
      this.#frame = document.createElement("iframe");
      this.#frame.loading = "lazy";
      this.#frame.allow = "fullscreen; autoplay";
      root.append(style, this.#frame);
    }
    window.addEventListener("message", this.#onMessage);
    this.#update();
  }

  disconnectedCallback() {
    window.removeEventListener("message", this.#onMessage);
  }

  attributeChangedCallback(name, before, after) {
    if (!this.#frame || before === after) return;
    // Looping changes without reloading the file.
    if (name === "loop" && this.#state.duration >= 0) this.#send("loop", { value: after !== null });
    else this.#update();
  }

  play() {
    this.#send("play");
  }

  pause() {
    this.#send("pause");
  }

  seek(seconds) {
    this.#send("seek", { time: Number(seconds) });
  }

  get currentTime() {
    return this.#state.time;
  }

  set currentTime(seconds) {
    this.seek(seconds);
  }

  get duration() {
    return this.#state.duration;
  }

  get paused() {
    return !this.#state.playing;
  }

  get ended() {
    return this.#state.ended;
  }

  get muted() {
    return this.#state.muted;
  }

  set muted(value) {
    this.#send("mute", { value: Boolean(value) });
  }

  get loop() {
    return this.hasAttribute("loop");
  }

  set loop(value) {
    this.toggleAttribute("loop", Boolean(value));
  }

  // The frame's address, label and proportion, from the attributes.
  #update() {
    this.#frame.title = this.getAttribute("label") ?? "Animación";
    const ratio = this.getAttribute("ratio");
    if (ratio) this.style.setProperty("--gaanim-ratio", ratio);
    const src = this.getAttribute("src");
    if (!src) return;
    const url = new URL(PLAYER);
    url.search = `src=${encodeURIComponent(new URL(src, document.baseURI).href)}`;
    for (const name of OPTIONS) {
      const value = this.getAttribute(name);
      if (value !== null) url.search += value === "" ? `&${name}` : `&${name}=${encodeURIComponent(value)}`;
    }
    const start = this.getAttribute("start");
    if (start) url.hash = `t=${encodeURIComponent(start)}`;
    if (this.#frame.src !== url.href) this.#frame.src = url.href;
  }

  #send(command, extra = {}) {
    this.#frame?.contentWindow?.postMessage({ type: "gaanim:command", command, ...extra }, PLAYER.origin);
  }

  #heard(event) {
    const data = event.data;
    if (!this.#frame || event.source !== this.#frame.contentWindow || data?.type !== "gaanim:status") return;
    for (const key of ["time", "duration", "playing", "ended", "muted"]) {
      if (data[key] !== undefined) this.#state[key] = data[key];
    }
    if (data.width > 0 && data.height > 0 && !this.hasAttribute("ratio")) {
      this.style.setProperty("--gaanim-ratio", `${data.width} / ${data.height}`);
    }
    this.dispatchEvent(new CustomEvent(data.kind, { detail: data }));
  }
}

if (!customElements.get("gaanim-player")) customElements.define("gaanim-player", GaanimPlayer);
