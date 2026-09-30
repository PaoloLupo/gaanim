// Gaanim poll relay.
//
// A presentation opens a session under a six-character code and holds a
// secret key for it. The audience scans a QR code to /s/<code>, a page that
// shows the current question and sends one vote per phone. The presentation
// opens and closes questions and reads the counts with its key.
//
//   GET    /s/<code>          voting page (public)
//   GET    /s/<code>/poll     current question, without counts (public)
//   POST   /s/<code>/vote     {poll, option, voter} (public)
//   PUT    /s/<code>/poll     {question, options} -> {id}   (presenter)
//   DELETE /s/<code>/poll     close the question            (presenter)
//   GET    /s/<code>/results  {id, counts, total}           (presenter)
//
// Votes are anonymous: a voter is a random id the page keeps in the phone's
// storage. A session and its votes are deleted after SESSION_TTL_MS without
// activity.

import { DurableObject } from "cloudflare:workers";

const CODE = /^[A-HJ-NP-Z2-9]{6}$/;
const VOTER = /^[0-9a-f]{32}$/;
const KEY = /^[0-9a-f]{64}$/;
const MAX_OPTIONS = 6;
const MAX_QUESTION = 300;
const MAX_OPTION = 120;
const SESSION_TTL_MS = 12 * 60 * 60 * 1000;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const parts = url.pathname.split("/").filter(Boolean);
    if (parts.length === 0) {
      return json({ relay: "gaanim", version: 1 });
    }
    const code = (parts[1] ?? "").toUpperCase();
    if (parts[0] !== "s" || !CODE.test(code) || parts.length > 3) {
      return json({ error: "not found" }, 404);
    }
    if (parts.length === 2) {
      return request.method === "GET" ? page(code) : json({ error: "method" }, 405);
    }
    const session = env.SESSIONS.get(env.SESSIONS.idFromName(code));
    const route = `${request.method} ${parts[2]}`;
    try {
      switch (route) {
        case "GET poll":
          return json(await session.current());
        case "POST vote":
          return reply(await session.vote(await body(request)));
        case "PUT poll":
          return reply(await session.open(bearer(request), await body(request)));
        case "DELETE poll":
          return reply(await session.close(bearer(request)));
        case "GET results":
          return reply(await session.results(bearer(request)));
        default:
          return json({ error: "not found" }, 404);
      }
    } catch (error) {
      return json({ error: String(error?.message ?? error) }, 400);
    }
  },
};

export class PollSession extends DurableObject {
  async current() {
    const poll = await this.ctx.storage.get("poll");
    if (!poll || !poll.open) {
      return { open: false };
    }
    return { open: true, id: poll.id, question: poll.question, options: poll.options };
  }

  async vote(input) {
    const poll = await this.ctx.storage.get("poll");
    if (!poll || !poll.open || input?.poll !== poll.id) {
      return { status: 409, body: { error: "this question is closed" } };
    }
    const option = input.option;
    if (!Number.isInteger(option) || option < 0 || option >= poll.options.length) {
      return { status: 400, body: { error: "unknown answer" } };
    }
    if (typeof input.voter !== "string" || !VOTER.test(input.voter)) {
      return { status: 400, body: { error: "invalid voter" } };
    }
    const key = `vote:${input.voter}`;
    const previous = await this.ctx.storage.get(key);
    const counts = (await this.ctx.storage.get("counts")) ?? poll.options.map(() => 0);
    if (previous && previous.poll === poll.id) {
      if (previous.option === option) {
        return { status: 200, body: { ok: true } };
      }
      counts[previous.option] = Math.max(0, counts[previous.option] - 1);
    }
    counts[option] += 1;
    await this.ctx.storage.put({ [key]: { poll: poll.id, option }, counts });
    await this.touch();
    return { status: 200, body: { ok: true } };
  }

  async open(key, input) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    const question = text(input?.question, MAX_QUESTION);
    const options = Array.isArray(input?.options)
      ? input.options.map((option) => text(option, MAX_OPTION))
      : [];
    if (!question || options.length < 2 || options.length > MAX_OPTIONS || options.some((o) => !o)) {
      return { status: 400, body: { error: "invalid question" } };
    }
    const id = crypto.randomUUID();
    // Earlier votes keep their poll id, so they no longer count.
    await this.ctx.storage.put({
      poll: { id, question, options, open: true },
      counts: options.map(() => 0),
    });
    await this.touch();
    return { status: 200, body: { id } };
  }

  async close(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const poll = await this.ctx.storage.get("poll");
    if (poll && poll.open) {
      await this.ctx.storage.put("poll", { ...poll, open: false });
    }
    await this.touch();
    return { status: 200, body: { ok: true } };
  }

  async results(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const poll = await this.ctx.storage.get("poll");
    if (!poll) {
      return { status: 200, body: { id: null, counts: [], total: 0 } };
    }
    const counts = (await this.ctx.storage.get("counts")) ?? poll.options.map(() => 0);
    const total = counts.reduce((sum, count) => sum + count, 0);
    return { status: 200, body: { id: poll.id, open: poll.open, counts, total } };
  }

  // The first presenter request claims the session with its key.
  async authorize(key, claim) {
    if (typeof key !== "string" || !KEY.test(key)) {
      return { status: 401, body: { error: "missing presenter key" } };
    }
    const hash = await sha256(key);
    const stored = await this.ctx.storage.get("key");
    if (!stored) {
      if (!claim) return { status: 401, body: { error: "unknown session" } };
      await this.ctx.storage.put("key", hash);
      return null;
    }
    return stored === hash ? null : { status: 403, body: { error: "wrong presenter key" } };
  }

  async touch() {
    await this.ctx.storage.setAlarm(Date.now() + SESSION_TTL_MS);
  }

  async alarm() {
    await this.ctx.storage.deleteAll();
  }
}

function text(value, limit) {
  return typeof value === "string" ? value.trim().slice(0, limit) : "";
}

function bearer(request) {
  const header = request.headers.get("authorization") ?? "";
  return header.startsWith("Bearer ") ? header.slice(7).trim() : null;
}

async function body(request) {
  const raw = await request.text();
  if (raw.length > 8192) throw new Error("request too large");
  return raw ? JSON.parse(raw) : {};
}

async function sha256(value) {
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(value));
  return [...new Uint8Array(digest)].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

function reply(result) {
  return json(result.body, result.status);
}

function json(value, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}

function page(code) {
  return new Response(VOTE_PAGE.replaceAll("__CODE__", code), {
    headers: { "content-type": "text/html; charset=utf-8", "cache-control": "no-store" },
  });
}

const VOTE_PAGE = `<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Gaanim · __CODE__</title>
<style>
  :root { color-scheme: dark; --bg: #11131a; --card: #1b1e28; --text: #f1f2f6; --muted: #a2a7b4; }
  * { box-sizing: border-box; }
  body { margin: 0; min-height: 100vh; background: var(--bg); color: var(--text);
         font: 17px/1.4 system-ui, -apple-system, "Segoe UI", sans-serif; }
  main { max-width: 560px; margin: 0 auto; padding: 20px 16px 32px; }
  header { display: flex; justify-content: space-between; color: var(--muted); font-size: 14px; }
  h1 { font-size: 24px; margin: 20px 0; }
  .options { display: grid; gap: 12px; }
  button { display: flex; gap: 12px; align-items: center; width: 100%; min-height: 64px;
           padding: 14px 16px; border: 3px solid transparent; border-radius: 14px;
           color: #fff; font: inherit; font-weight: 600; text-align: left; cursor: pointer; }
  button .letter { flex: none; width: 34px; height: 34px; border-radius: 50%; display: grid;
                   place-items: center; background: rgba(0,0,0,.25); }
  button.chosen { border-color: #fff; }
  button:disabled { opacity: .6; }
  .status { margin-top: 18px; color: var(--muted); text-align: center; min-height: 1.4em; }
  .waiting { margin-top: 30vh; text-align: center; color: var(--muted); }
</style>
</head>
<body>
<main>
  <header><span>Gaanim</span><span>__CODE__</span></header>
  <div id="app"><p class="waiting" id="waiting"></p></div>
</main>
<script>
const CODE = "__CODE__";
const COLORS = ["#e2475b", "#3b7ddd", "#d9a21b", "#2f9e5b", "#8e5bd6", "#1f9ea8"];
const es = (navigator.language || "es").toLowerCase().startsWith("es");
const T = es
  ? { wait: "Esperando la siguiente pregunta…", sent: "Voto registrado. Puedes cambiarlo mientras la pregunta siga abierta.",
      closed: "La pregunta se cerró.", offline: "Sin conexión; reintentando…", sending: "Enviando…" }
  : { wait: "Waiting for the next question…", sent: "Vote received. You can change it while the question is open.",
      closed: "The question closed.", offline: "Offline; retrying…", sending: "Sending…" };
function storage(key, value) {
  try {
    if (value === undefined) return localStorage.getItem(key);
    localStorage.setItem(key, value);
  } catch (_) {}
  return null;
}
let voter = storage("gaanim-voter");
if (!voter || !/^[0-9a-f]{32}$/.test(voter)) {
  voter = [...crypto.getRandomValues(new Uint8Array(16))].map((b) => b.toString(16).padStart(2, "0")).join("");
  storage("gaanim-voter", voter);
}
const app = document.getElementById("app");
let shown = null;
function render(poll) {
  if (!poll.open) {
    shown = null;
    app.innerHTML = '<p class="waiting"></p>';
    app.firstChild.textContent = T.wait;
    return;
  }
  if (shown === poll.id) return;
  shown = poll.id;
  app.innerHTML = '<h1></h1><div class="options"></div><p class="status"></p>';
  app.querySelector("h1").textContent = poll.question;
  const list = app.querySelector(".options");
  const status = app.querySelector(".status");
  const chosen = storage("gaanim-vote-" + poll.id);
  poll.options.forEach((option, index) => {
    const button = document.createElement("button");
    button.style.background = COLORS[index % COLORS.length];
    button.innerHTML = '<span class="letter"></span><span class="label"></span>';
    button.querySelector(".letter").textContent = String.fromCharCode(65 + index);
    button.querySelector(".label").textContent = option;
    if (chosen === String(index)) button.classList.add("chosen");
    button.onclick = async () => {
      status.textContent = T.sending;
      try {
        const response = await fetch("/s/" + CODE + "/vote", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ poll: poll.id, option: index, voter }),
        });
        if (response.status === 409) { status.textContent = T.closed; return; }
        if (!response.ok) throw new Error(String(response.status));
        storage("gaanim-vote-" + poll.id, String(index));
        list.querySelectorAll("button").forEach((b) => b.classList.remove("chosen"));
        button.classList.add("chosen");
        status.textContent = T.sent;
      } catch (_) {
        status.textContent = T.offline;
      }
    };
    list.appendChild(button);
  });
  if (chosen !== null) status.textContent = T.sent;
}
async function refresh() {
  try {
    const response = await fetch("/s/" + CODE + "/poll", { cache: "no-store" });
    render(await response.json());
  } catch (_) {
    if (shown === null) app.firstChild.textContent = T.offline;
  }
  setTimeout(refresh, 2000);
}
document.getElementById("waiting").textContent = T.wait;
refresh();
</script>
</body>
</html>`;
