// Players' characters: drawn from the parts in avatar-parts.json, which the
// relay validates against and Gaanim draws in the scene. A character is
// [body, color, eyes, mouth, extra], indexes into the catalog's lists.
//
// `avatarPose` is how a character looks at a time: breathing, blinking and
// an expression. It follows the motion rules in the catalog's `about`, the
// same Gaanim implements (gaanim_objects::character), so a character moves
// the same on a phone and in the presentation.
"use strict";

const SVG_NS = "http://www.w3.org/2000/svg";
/** Room around the 100-unit square for hats, ears and headphones. */
const AVATAR_VIEWBOX = "-6 -30 112 126";

/** The part lists a character indexes, in its order. */
const AVATAR_PARTS = ["bodies", "colors", "eyes", "mouths", "extras"];

let avatarCatalog = null;

/** The catalog, fetched once. */
async function loadAvatarCatalog() {
  if (!avatarCatalog) {
    avatarCatalog = fetch("/avatar-parts.json").then((response) => {
      if (!response.ok) throw new Error(String(response.status));
      return response.json();
    });
  }
  return avatarCatalog;
}

/** Whether `avatar` is a character of `catalog`. */
function isAvatar(catalog, avatar) {
  return (
    Array.isArray(avatar) &&
    avatar.length === AVATAR_PARTS.length &&
    avatar.every(
      (index, part) =>
        Number.isInteger(index) && index >= 0 && index < catalog[AVATAR_PARTS[part]].length,
    )
  );
}

/** A character picked at random. Mouths and extras are left out a little
 * less often than any single one is chosen, so most characters have both. */
function randomAvatar(catalog) {
  const pick = (count, skipNone = false) => {
    const start = skipNone && count > 1 && Math.random() < 0.85 ? 1 : 0;
    return start + Math.floor(Math.random() * (count - start));
  };
  return [
    pick(catalog.bodies.length),
    pick(catalog.colors.length),
    pick(catalog.eyes.length),
    pick(catalog.mouths.length, true),
    pick(catalog.extras.length, true),
  ];
}

// --- Motion -----------------------------------------------------------------

/** FNV-1a of `text`'s UTF-8 bytes: the seed of a character's blinking. */
function avatarSeed(text) {
  let hash = 0x811c9dc5;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= byte;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return hash;
}

/** 2D affine matrices as [a, b, c, d, e, f], like SVG's matrix(). */
function multiply([a1, b1, c1, d1, e1, f1], [a2, b2, c2, d2, e2, f2]) {
  return [
    a1 * a2 + c1 * b2,
    b1 * a2 + d1 * b2,
    a1 * c2 + c1 * d2,
    b1 * c2 + d1 * d2,
    a1 * e2 + c1 * f2 + e1,
    b1 * e2 + d1 * f2 + f1,
  ];
}

/** translate(pivot + (dx, dy)) rotate(rot) scale(sx, sy) translate(-pivot). */
function motionMatrix([px, py], dx, dy, sx, sy, rot) {
  const radians = (rot * Math.PI) / 180;
  const cos = Math.cos(radians);
  const sin = Math.sin(radians);
  const [a, b, c, d] = [cos * sx, sin * sx, -sin * sy, cos * sy];
  return [a, b, c, d, px + dx - (a * px + c * py), py + dy - (b * px + d * py)];
}

/** A motion's [dx, dy, sx, sy, rot] at progress `p`. */
function motionAt(motion, p) {
  const keys = motion.keys;
  let index = 0;
  while (index < keys.length - 2 && p > keys[index + 1][0]) index += 1;
  const [from, to] = [keys[index], keys[Math.min(index + 1, keys.length - 1)]];
  const span = to[0] - from[0];
  let s = span > 0 ? Math.min(Math.max((p - from[0]) / span, 0), 1) : 1;
  if (motion.ease !== "linear") s = s * s * (3 - 2 * s);
  return [1, 2, 3, 4, 5].map((i) => from[i] + (to[i] - from[i]) * s);
}

/** A face part by name: an expression's own, else a character's. */
function facePart(catalog, list, name) {
  return catalog.faces[name] ?? catalog[list].find((part) => part.name === name) ?? null;
}

/** The whole drawing's matrix at `t`: breathing, then the expression's motion. */
function wholeAt(catalog, base, t, expression) {
  const breathe = catalog.idle.breathe;
  const breath = breathe.amount * Math.sin((2 * Math.PI * t) / breathe.period);
  let whole = motionMatrix(base, 0, 0, 1 - breath / 2, 1 + breath, 0);
  const shown = expression && catalog.expressions[expression.name];
  if (shown && shown.motion) {
    const since = t - expression.start;
    if (since >= 0 && (expression.loop || since < shown.hold)) {
      const motion = catalog.motions[shown.motion];
      const p = expression.loop
        ? (since % motion.duration) / motion.duration
        : Math.min(since / motion.duration, 1);
      whole = multiply(motionMatrix(base, ...motionAt(motion, p)), whole);
    }
  }
  return whole;
}

/** How far a mass hanging from a point trails behind it: `positions[j]` is
 * where the point was `j * step` seconds ago. See the catalog's `about`. */
function followLag(positions, step, frequency, damping) {
  const omega = 2 * Math.PI * frequency;
  const damped = omega * Math.sqrt(Math.max(1 - damping * damping, 1e-6));
  const lag = [0, 0];
  for (let j = 1; j < positions.length - 1; j += 1) {
    const u = j * step;
    const weight = (Math.exp(-damping * omega * u) * Math.sin(damped * u)) / damped / step;
    const [a, b, c] = [positions[j - 1], positions[j], positions[j + 1]];
    lag[0] -= weight * (a[0] - 2 * b[0] + c[0]);
    lag[1] -= weight * (a[1] - 2 * b[1] + c[1]);
  }
  return lag;
}

/**
 * How `avatar` looks at time `t`: `expression` is null or
 * {name, start, loop}; `drive` is null or {lag: [x, y], look: [x, y]}, what
 * a presentation moving the character adds. Returns the layers to draw and
 * their matrices: `whole` applies to everything, the eyes', the extra's and
 * each effect's own after it.
 */
function avatarPose(catalog, avatar, seed, t, expression = null, drive = null) {
  const [bodyIndex, , eyesIndex, mouthIndex, extraIndex] = avatar;
  const body = catalog.bodies[bodyIndex];
  const extra = catalog.extras[extraIndex];
  const ownEyes = catalog.eyes[eyesIndex];
  const idle = catalog.idle;
  const base = [50, body.base];

  const whole = wholeAt(catalog, base, t, expression);

  const since = expression ? t - expression.start : 0;
  const shown = expression && catalog.expressions[expression.name];
  const active = shown && since >= 0 && (expression.loop || since < shown.hold);
  let eyes = ownEyes;
  let mouth = catalog.mouths[mouthIndex];
  const effects = [];
  if (active) {
    if (shown.eyes && !ownEyes.keep) eyes = facePart(catalog, "eyes", shown.eyes) ?? eyes;
    if (shown.mouth) mouth = facePart(catalog, "mouths", shown.mouth) ?? mouth;
    for (const name of shown.effects ?? []) {
      const effect = catalog.effects[name];
      const motion = catalog.motions[effect.motion];
      const p = (since % motion.duration) / motion.duration;
      const anchor = body[effect.anchor] ?? body.face;
      effects.push({
        layers: effect.layers,
        matrix: multiply([1, 0, 0, 1, 0, anchor], motionMatrix(effect.pivot, ...motionAt(motion, p))),
      });
    }
  }

  // Blinking: only the character's own eyes, and only those that can.
  let blink = 1;
  if (eyes === ownEyes && ownEyes.blink !== false) {
    const every = idle.blink.every;
    const period = every[0] + ((every[1] - every[0]) * (seed & 0xffff)) / 65535;
    const offset = (period * ((seed >>> 16) & 0xffff)) / 65535;
    const x = (t + offset) % period;
    if (x < idle.blink.duration) blink = 1 - 0.9 * Math.sin((Math.PI * x) / idle.blink.duration);
  }
  const face = body.face - 50;
  const clamp = (value, low, high) => Math.min(Math.max(value, low), high);
  const look = drive?.look ?? [0, 0];
  const lookAt = [clamp(look[0], -1, 1) * idle.look[0], clamp(look[1], -1, 1) * idle.look[1]];

  // Follow-through: the extra hangs from the body like a damped spring.
  const anchor = body[extra.anchor] ?? body.top;
  let extraMatrix = [1, 0, 0, 1, 0, anchor];
  if (extra.follow) {
    const follow = idle.follow;
    const pivot = extra.pivot ?? [50, 0];
    const positions = [];
    for (let j = 0; j <= follow.samples + 1; j += 1) {
      const m = wholeAt(catalog, base, t - j * follow.step, expression);
      const [x, y] = [pivot[0], anchor + pivot[1]];
      positions.push([m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]);
    }
    const own = followLag(positions, follow.step, follow.frequency, follow.damping);
    const added = drive?.lag ?? [0, 0];
    const lag = [own[0] + added[0], own[1] + added[1]];
    const swing = clamp(extra.follow * lag[0], -follow.max, follow.max);
    const stretch = clamp(1 - extra.follow * follow.stretch * lag[1], 0.7, 1.3);
    extraMatrix = multiply(extraMatrix, motionMatrix(pivot, 0, 0, 1, stretch, swing));
  }

  return {
    whole,
    body: body.layers,
    extra: extra.layers,
    extraBehind: Boolean(extra.behind),
    extraMatrix,
    eyes: eyes.layers,
    eyesMatrix: multiply(
      [1, 0, 0, 1, lookAt[0], face + lookAt[1]],
      motionMatrix([50, 50], 0, 0, 1, blink, 0),
    ),
    mouth: mouth.layers,
    face,
    effects,
  };
}

// --- Drawing ------------------------------------------------------------------

function avatarSvg(size, label) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", AVATAR_VIEWBOX);
  svg.setAttribute("height", String(size));
  svg.setAttribute("width", String(Math.round((size * 112) / 126)));
  svg.classList.add("avatar");
  if (label) {
    svg.setAttribute("role", "img");
    svg.setAttribute("aria-label", label);
  } else {
    svg.setAttribute("aria-hidden", "true");
  }
  return svg;
}

/** A <g> of `layers` painted for `avatar`. */
function layerGroup(catalog, avatar, layers) {
  const color = catalog.colors[avatar[1]];
  const paint = (name) =>
    name === "body" ? color : name === "ink" ? catalog.ink : name === "white" ? catalog.white : name;
  const g = document.createElementNS(SVG_NS, "g");
  for (const layer of layers) {
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("d", layer.d);
    if (layer.stroke) {
      path.setAttribute("fill", "none");
      path.setAttribute("stroke", paint(layer.stroke));
      path.setAttribute("stroke-width", String(layer.width ?? 3));
      path.setAttribute("stroke-linecap", "round");
      path.setAttribute("stroke-linejoin", "round");
    } else {
      path.setAttribute("fill", paint(layer.fill));
    }
    g.append(path);
  }
  return g;
}

const matrixAttribute = (m) => `matrix(${m.map((value) => value.toFixed(4)).join(" ")})`;

/** Put the drawing of `pose` into `svg`, rebuilding its parts only when the
 * face or the effects changed since `previous`. */
function paintPose(catalog, avatar, svg, pose, previous) {
  const same =
    previous &&
    previous.eyes === pose.eyes &&
    previous.mouth === pose.mouth &&
    previous.effects.length === pose.effects.length &&
    previous.effects.every((effect, index) => effect.layers === pose.effects[index].layers);
  if (!same) {
    const whole = document.createElementNS(SVG_NS, "g");
    const extra = layerGroup(catalog, avatar, pose.extra);
    extra.dataset.part = "extra";
    if (pose.extraBehind) whole.append(extra);
    whole.append(layerGroup(catalog, avatar, pose.body));
    const eyes = layerGroup(catalog, avatar, pose.eyes);
    eyes.dataset.part = "eyes";
    whole.append(eyes);
    const mouth = layerGroup(catalog, avatar, pose.mouth);
    mouth.setAttribute("transform", `translate(0 ${pose.face})`);
    whole.append(mouth);
    if (!pose.extraBehind) whole.append(extra);
    for (const effect of pose.effects) {
      const g = layerGroup(catalog, avatar, effect.layers);
      g.dataset.part = "effect";
      whole.append(g);
    }
    svg.replaceChildren(whole);
  }
  const whole = svg.firstElementChild;
  whole.setAttribute("transform", matrixAttribute(pose.whole));
  whole.querySelector('[data-part="eyes"]').setAttribute("transform", matrixAttribute(pose.eyesMatrix));
  whole.querySelector('[data-part="extra"]').setAttribute("transform", matrixAttribute(pose.extraMatrix));
  whole.querySelectorAll('[data-part="effect"]').forEach((g, index) => {
    g.setAttribute("transform", matrixAttribute(pose.effects[index].matrix));
  });
}

/** A still <svg> of `avatar`, `size` pixels high, with `label` for screen
 * readers (or hidden from them without one). */
function drawAvatar(catalog, avatar, size, label = "") {
  const svg = avatarSvg(size, label);
  paintPose(catalog, avatar, svg, avatarPose(catalog, avatar, 0, 0), null);
  return svg;
}

/**
 * A living `avatar`: it breathes and blinks, seeded by `name`, and
 * `express(name, {loop})` plays an expression. `element` is its <svg>;
 * `stop()` ends its animation. Reduced motion keeps expressions' faces but
 * not their movement.
 */
function animateAvatar(catalog, avatar, size, name, label = "") {
  const svg = avatarSvg(size, label);
  const seed = avatarSeed(name);
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const origin = performance.now();
  let expression = null;
  let previous = null;
  let frame = 0;
  const now = () => (performance.now() - origin) / 1000;
  const draw = () => {
    const t = now();
    let pose = avatarPose(catalog, avatar, seed, still ? 0 : t, expression);
    if (still && expression) {
      // The face of the expression, without its movement.
      const moving = avatarPose(catalog, avatar, seed, t, expression);
      pose = { ...moving, whole: [1, 0, 0, 1, 0, 0], effects: [] };
    }
    paintPose(catalog, avatar, svg, pose, previous);
    previous = pose;
    frame = requestAnimationFrame(draw);
  };
  frame = requestAnimationFrame(draw);
  paintPose(catalog, avatar, svg, avatarPose(catalog, avatar, seed, 0), null);
  return {
    element: svg,
    express(expressionName, { loop = false } = {}) {
      if (catalog.expressions[expressionName]) {
        expression = { name: expressionName, start: now(), loop };
      }
    },
    stop() {
      cancelAnimationFrame(frame);
    },
  };
}
