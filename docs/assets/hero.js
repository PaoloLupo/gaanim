// Home hero scene. The Gaanim symbol comes alive: a square tweens into a
// circle, leaving onion-skin frames spaced by its easing; the circle becomes a
// unit circle whose turning point traces sin θ; a timeline playhead runs
// underneath. Colours come from the hero's --anim-* custom properties, so the
// scene follows the site theme. It pauses off-screen and stays still when the
// visitor prefers reduced motion.

const hero = document.querySelector(".home-hero");
const canvas = hero && hero.querySelector(".home-hero-canvas");
const stage = hero && hero.querySelector(".home-hero-stage");

// Beat boundaries in seconds; the scene loops after the last one.
const TWEEN_END = 2.2;
const MORPH_END = 3.0;
const TRACE_END = 9.0;
const HOLD_END = 10.0;
const LOOP = 11.0;
const KEYFRAMES = [0, TWEEN_END, MORPH_END, TRACE_END, HOLD_END, LOOP];
const TURNS = 1.5;
const GHOST_EVERY = 0.16; // seconds between onion-skin frames
const GHOST_LIFE = 1.1;
const INTRO = 1.4; // radial grid reveal on first paint
const STILL_TIME = 6.4; // frame shown when motion is reduced
const RULER = 46; // stage height reserved for the timeline
const MONO = '"VictorMono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

const clamp = (x) => Math.min(1, Math.max(0, x));
const smooth = (x) => (x < 0.5 ? 4 * x * x * x : 1 - (-2 * x + 2) ** 3 / 2);

if (canvas && stage) start();

function start() {
  const ctx = canvas.getContext("2d");
  const grid = document.createElement("canvas");
  const gctx = grid.getContext("2d");
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  const introStart = performance.now();
  let width = 0;
  let height = 0;
  let dpr = 1;
  let L = null; // layout
  let C = null; // colours
  let elapsed = 0;
  let last = 0;
  let frame = 0;
  let visible = true;

  function readColors() {
    const style = getComputedStyle(hero);
    const v = (name) => style.getPropertyValue(name).trim();
    C = {
      front: v("--anim-front"),
      violet: v("--anim-violet"),
      haze: v("--anim-haze"),
      grid: v("--anim-grid"),
      axis: v("--anim-axis"),
      label: v("--anim-label"),
      accent: v("--anim-accent"),
      surface: v("--anim-surface"),
    };
  }

  function measure() {
    const rect = hero.getBoundingClientRect();
    const box = stage.getBoundingClientRect();
    dpr = Math.min(window.devicePixelRatio || 1, 2);
    width = rect.width;
    height = rect.height;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const x = box.left - rect.left;
    const y = box.top - rect.top;
    const sceneH = box.height - RULER;
    const u = Math.max(12, Math.min(sceneH / 4.4, box.width / 8));
    const size = 1.5 * u;
    const travel = 1.6 * u;
    const cx = x + travel + size / 2 + 0.1 * u;
    const r = 1.1 * u;
    const waveStart = cx + r + 0.5 * u;
    const waveEnd = Math.min(width - 24, x + box.width + 16);
    L = {
      x, y, w: box.width, u, size, travel, cx, cy: y + sceneH / 2, r, waveStart, waveEnd,
      k: (waveEnd - waveStart) / (TURNS * 2 * Math.PI),
      rulerY: y + box.height - 14,
    };
    paintGrid();
  }

  // Unit grid aligned to the circle's centre, fading away from the scene.
  function paintGrid() {
    grid.width = canvas.width;
    grid.height = canvas.height;
    gctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    gctx.clearRect(0, 0, width, height);
    gctx.strokeStyle = C.grid;
    gctx.lineWidth = 1;
    gctx.beginPath();
    for (let gx = L.cx % L.u; gx < width; gx += L.u) {
      const px = Math.round(gx) + 0.5;
      gctx.moveTo(px, 0);
      gctx.lineTo(px, height);
    }
    for (let gy = L.cy % L.u; gy < height; gy += L.u) {
      const py = Math.round(gy) + 0.5;
      gctx.moveTo(0, py);
      gctx.lineTo(width, py);
    }
    gctx.stroke();
    gctx.globalCompositeOperation = "destination-in";
    const fade = gctx.createRadialGradient(L.cx, L.cy, 0, L.cx, L.cy, Math.max(width, height) * 0.85);
    fade.addColorStop(0, "rgba(0,0,0,1)");
    fade.addColorStop(0.5, "rgba(0,0,0,0.6)");
    fade.addColorStop(1, "rgba(0,0,0,0.1)");
    gctx.fillStyle = fade;
    gctx.fillRect(0, 0, width, height);
    gctx.globalCompositeOperation = "source-over";
  }

  function roundedSquare(cx, cy, size, radius) {
    const h = size / 2;
    const r = Math.min(radius, h);
    ctx.beginPath();
    ctx.moveTo(cx - h + r, cy - h);
    ctx.arcTo(cx + h, cy - h, cx + h, cy + h, r);
    ctx.arcTo(cx + h, cy + h, cx - h, cy + h, r);
    ctx.arcTo(cx - h, cy + h, cx - h, cy - h, r);
    ctx.arcTo(cx - h, cy - h, cx + h, cy - h, r);
    ctx.closePath();
  }

  // Square at the start, circle at the end; eased, so frames bunch at both ends.
  function tweenShape(time) {
    const p = smooth(clamp(time / TWEEN_END));
    return { x: L.cx - L.travel * (1 - p), radius: (p * L.size) / 2 };
  }

  function drawTween(t) {
    for (let tk = 0; tk <= Math.min(t, TWEEN_END); tk += GHOST_EVERY) {
      const age = t - tk;
      if (age >= GHOST_LIFE) continue;
      const shape = tweenShape(tk);
      ctx.globalAlpha = 0.55 * (1 - age / GHOST_LIFE);
      ctx.fillStyle = age < GHOST_LIFE / 2 ? C.violet : C.haze;
      roundedSquare(shape.x, L.cy, L.size, shape.radius);
      ctx.fill();
    }
    ctx.globalAlpha = 1;
    if (t < TWEEN_END) {
      const shape = tweenShape(t);
      ctx.globalAlpha = clamp(t / 0.25);
      ctx.fillStyle = C.front;
      roundedSquare(shape.x, L.cy, L.size, shape.radius);
      ctx.fill();
      ctx.globalAlpha = 1;
    }
  }

  function drawAxes(alpha) {
    const { cx, cy, r, u, waveStart, waveEnd, k } = L;
    ctx.globalAlpha = alpha;
    ctx.strokeStyle = C.axis;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(cx - r - 0.4 * u, cy);
    ctx.lineTo(waveEnd, cy);
    ctx.moveTo(cx, cy - r - 0.4 * u);
    ctx.lineTo(cx, cy + r + 0.4 * u);
    ctx.moveTo(waveStart, cy - r - 0.3 * u);
    ctx.lineTo(waveStart, cy + r + 0.3 * u);
    for (let m = 1; m <= TURNS * 2; m++) {
      const px = waveStart + k * Math.PI * m;
      ctx.moveTo(px, cy - 4);
      ctx.lineTo(px, cy + 4);
    }
    ctx.stroke();
    ctx.fillStyle = C.label;
    ctx.font = `11px ${MONO}`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (let m = 1; m <= TURNS * 2; m++) {
      ctx.fillText(m === 1 ? "π" : `${m}π`, waveStart + k * Math.PI * m, cy + 8);
    }
    ctx.globalAlpha = 1;
  }

  function drawTrace(t, alpha) {
    const { cx, cy, r, u, waveStart, k } = L;
    const q = smooth(clamp((t - TWEEN_END) / (MORPH_END - TWEEN_END)));
    const theta = TURNS * 2 * Math.PI * clamp((t - MORPH_END) / (TRACE_END - MORPH_END));
    const radius = L.size / 2 + (r - L.size / 2) * q;

    // Filled circle hands over to the unit circle's outline.
    if (q < 1) {
      ctx.globalAlpha = alpha * (1 - q);
      ctx.fillStyle = C.front;
      ctx.beginPath();
      ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
      ctx.fill();
    }
    ctx.globalAlpha = alpha * q;
    ctx.strokeStyle = C.violet;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, 2 * Math.PI);
    ctx.stroke();
    if (q < 1) {
      ctx.globalAlpha = 1;
      return;
    }

    const dot = { x: cx + r * Math.cos(theta), y: cy - r * Math.sin(theta) };
    const head = { x: waveStart + k * theta, y: dot.y };

    ctx.globalAlpha = alpha;
    ctx.strokeStyle = C.label;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.arc(cx, cy, 0.32 * u, 0, -(theta % (2 * Math.PI)) || 0, true);
    ctx.stroke();

    ctx.strokeStyle = C.violet;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(cx, cy);
    ctx.lineTo(dot.x, dot.y);
    ctx.stroke();

    ctx.setLineDash([3, 4]);
    ctx.strokeStyle = C.label;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(dot.x, dot.y);
    ctx.lineTo(head.x, head.y);
    ctx.stroke();
    ctx.setLineDash([]);

    ctx.strokeStyle = C.front;
    ctx.lineWidth = 2.5;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";
    ctx.beginPath();
    const steps = Math.max(1, Math.ceil(theta / 0.04));
    for (let i = 0; i <= steps; i++) {
      const a = (theta * i) / steps;
      const px = waveStart + k * a;
      const py = cy - r * Math.sin(a);
      if (i === 0) ctx.moveTo(px, py);
      else ctx.lineTo(px, py);
    }
    ctx.stroke();
    ctx.lineCap = "butt";

    ctx.fillStyle = C.front;
    for (const p of [dot, head]) {
      ctx.beginPath();
      ctx.arc(p.x, p.y, 4.5, 0, 2 * Math.PI);
      ctx.fill();
    }

    ctx.fillStyle = C.label;
    ctx.font = `12px ${MONO}`;
    ctx.textAlign = "left";
    ctx.textBaseline = "bottom";
    ctx.fillText("sin θ", Math.min(head.x + 9, L.waveEnd - 36), head.y - 8);
    ctx.globalAlpha = 1;
  }

  function drawTimeline(t) {
    const { x, w, rulerY } = L;
    const at = (time) => Math.round(x + (time / LOOP) * w);
    const clips = [
      [0, TWEEN_END, C.haze],
      [TWEEN_END, MORPH_END, C.violet],
      [MORPH_END, TRACE_END, C.front],
      [TRACE_END, LOOP, C.axis],
    ];
    for (const [a, b, color] of clips) {
      ctx.globalAlpha = 0.85;
      ctx.fillStyle = color;
      ctx.fillRect(at(a) + 2, rulerY - 12, at(b) - at(a) - 4, 4);
    }
    ctx.globalAlpha = 1;
    ctx.strokeStyle = C.axis;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(x, rulerY + 0.5);
    ctx.lineTo(x + w, rulerY + 0.5);
    for (let s = 0; s <= LOOP; s++) {
      ctx.moveTo(at(s) + 0.5, rulerY);
      ctx.lineTo(at(s) + 0.5, rulerY + (s % 5 === 0 ? 7 : 4));
    }
    ctx.stroke();
    for (const key of KEYFRAMES) {
      const kx = at(key) + 0.5;
      ctx.beginPath();
      ctx.moveTo(kx, rulerY - 4);
      ctx.lineTo(kx + 4, rulerY);
      ctx.lineTo(kx, rulerY + 4);
      ctx.lineTo(kx - 4, rulerY);
      ctx.closePath();
      if (t >= key) {
        ctx.fillStyle = C.accent;
        ctx.fill();
      } else {
        ctx.fillStyle = C.surface;
        ctx.fill();
        ctx.strokeStyle = C.axis;
        ctx.stroke();
      }
    }
    const px = at(t) + 0.5;
    ctx.strokeStyle = C.violet;
    ctx.lineWidth = 1.5;
    ctx.beginPath();
    ctx.moveTo(px, rulerY - 22);
    ctx.lineTo(px, rulerY + 7);
    ctx.stroke();
    ctx.fillStyle = C.violet;
    ctx.fillRect(px - 3.5, rulerY - 28, 7, 7);

    ctx.fillStyle = C.label;
    ctx.font = `11px ${MONO}`;
    ctx.textAlign = "right";
    ctx.textBaseline = "bottom";
    ctx.fillText(`t = ${t.toFixed(2)} s`, x + w, rulerY - 18);
  }

  function draw(t, now) {
    if (!L) return;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const intro = clamp((now - introStart) / (INTRO * 1000));
    if (intro < 1) {
      ctx.save();
      ctx.beginPath();
      ctx.arc(L.cx, L.cy, smooth(intro) * Math.hypot(width, height), 0, 2 * Math.PI);
      ctx.clip();
      ctx.drawImage(grid, 0, 0, width, height);
      ctx.restore();
    } else {
      ctx.drawImage(grid, 0, 0, width, height);
    }
    const alpha = t > HOLD_END ? 1 - smooth((t - HOLD_END) / (LOOP - HOLD_END)) : 1;
    drawAxes(alpha * smooth(clamp((t - TWEEN_END) / (MORPH_END - TWEEN_END))));
    ctx.globalAlpha = alpha;
    drawTween(t);
    ctx.globalAlpha = 1;
    if (t >= TWEEN_END) drawTrace(t, alpha);
    drawTimeline(t);
  }

  function still() {
    draw(STILL_TIME, Infinity);
  }

  function tick(now) {
    frame = 0;
    if (!visible || document.hidden) {
      last = 0;
      return;
    }
    if (last) elapsed = (elapsed + Math.min(0.1, (now - last) / 1000)) % LOOP;
    last = now;
    draw(elapsed, now);
    frame = requestAnimationFrame(tick);
  }

  function play() {
    if (reduced.matches) still();
    else if (!frame) frame = requestAnimationFrame(tick);
  }

  function restyle() {
    readColors();
    if (L) paintGrid();
    if (reduced.matches) still();
  }

  readColors();
  measure();
  play();
  new ResizeObserver(() => {
    measure();
    if (reduced.matches) still();
    else draw(elapsed, performance.now());
  }).observe(hero);
  new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    if (visible) play();
  }).observe(hero);
  new MutationObserver(restyle).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", restyle);
  reduced.addEventListener("change", play);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) play();
  });
  if (document.fonts) document.fonts.ready.then(() => reduced.matches && still());
}
