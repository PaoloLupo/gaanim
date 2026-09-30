// Gaanim relay.
//
// A presentation opens a session under a six-character code and holds a
// secret key for it. Each poll has an id the presentation chooses, so a poll
// keeps its votes when the presentation comes back to it. The audience scans a QR code to /s/<code>, a page that
// shows the current question and sends one vote per phone. The presentation
// opens and closes questions and reads the counts with its key.
//
//   GET    /                  join page: type a code (static asset)
//   GET    /s/<code>          voting page (public)
//   GET    /s/<code>/poll     current question, without counts (public)
//   POST   /s/<code>/vote     {poll, option, voter} (public)
//   PUT    /s/<code>/poll     {id, question, options}       (presenter)
//   DELETE /s/<code>/poll     close the open question       (presenter)
//   GET    /s/<code>/results  {current, polls: {id: {open, counts, total}}}
//   GET    /health            {relay, version}
//
// Votes are anonymous: a voter is a random id the page keeps in the phone's
// storage. A session and its votes are deleted after SESSION_TTL_MS without
// activity.

import { DurableObject } from "cloudflare:workers";

const CODE = /^[A-HJ-NP-Z2-9]{6}$/;
const VOTER = /^[0-9a-f]{32}$/;
const KEY = /^[0-9a-f]{64}$/;
/** Poll ids are chosen by the presentation: stable across presentations. */
const POLL_ID = /^[a-z0-9][a-z0-9-]{0,63}$/;
const MAX_OPTIONS = 6;
const MAX_QUESTION = 300;
const MAX_OPTION = 120;
const MAX_BODY = 8192;
const SESSION_TTL_MS = 12 * 60 * 60 * 1000;
const API_VERSION = 2;

/** Headers every page and API response carries. */
const SECURITY_HEADERS = {
  "x-content-type-options": "nosniff",
  "referrer-policy": "no-referrer",
  "x-frame-options": "DENY",
};

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const parts = url.pathname.split("/").filter(Boolean);
    if (url.pathname === "/health") {
      return json({ relay: "gaanim", version: API_VERSION });
    }
    const code = (parts[1] ?? "").toUpperCase();
    if (parts[0] !== "s" || !CODE.test(code) || parts.length > 3) {
      return json({ error: "not found" }, 404);
    }
    if (parts.length === 2) {
      if (request.method !== "GET") return json({ error: "method not allowed" }, 405);
      // Short, shareable address: /s/abc234 → /s/ABC234.
      if (parts[1] !== code) return Response.redirect(`${url.origin}/s/${code}`, 302);
      return votePage(env, url);
    }
    const session = env.SESSIONS.get(env.SESSIONS.idFromName(code));
    try {
      switch (`${request.method} ${parts[2]}`) {
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
  // Storage: "key" (presenter key hash), "current" (open poll id or absent),
  // "poll:<id>" ({question, options, counts}) and "vote:<id>:<voter>"
  // (the answer index). Every poll keeps its votes, so a presentation that
  // comes back to a question finds them again.

  async current() {
    const id = await this.ctx.storage.get("current");
    const poll = id && (await this.ctx.storage.get(`poll:${id}`));
    if (!poll) {
      return { open: false };
    }
    return { open: true, id, question: poll.question, options: poll.options };
  }

  async vote(input) {
    const id = await this.ctx.storage.get("current");
    if (!id || input?.poll !== id) {
      return { status: 409, body: { error: "this question is closed" } };
    }
    const poll = await this.ctx.storage.get(`poll:${id}`);
    const option = input.option;
    if (!Number.isInteger(option) || option < 0 || option >= poll.options.length) {
      return { status: 400, body: { error: "unknown answer" } };
    }
    if (typeof input.voter !== "string" || !VOTER.test(input.voter)) {
      return { status: 400, body: { error: "invalid voter" } };
    }
    const key = `vote:${id}:${input.voter}`;
    const previous = await this.ctx.storage.get(key);
    if (previous === option) {
      return { status: 200, body: { ok: true } };
    }
    if (previous !== undefined) {
      poll.counts[previous] = Math.max(0, poll.counts[previous] - 1);
    }
    poll.counts[option] += 1;
    await this.ctx.storage.put({ [key]: option, [`poll:${id}`]: poll });
    await this.touch();
    return { status: 200, body: { ok: true } };
  }

  async open(key, input) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    const id = typeof input?.id === "string" ? input.id : "";
    const question = text(input?.question, MAX_QUESTION);
    const options = Array.isArray(input?.options)
      ? input.options.map((option) => text(option, MAX_OPTION))
      : [];
    if (
      !POLL_ID.test(id) ||
      !question ||
      options.length < 2 ||
      options.length > MAX_OPTIONS ||
      options.some((option) => !option)
    ) {
      return { status: 400, body: { error: "invalid question" } };
    }
    const existing = await this.ctx.storage.get(`poll:${id}`);
    const same =
      existing &&
      existing.question === question &&
      existing.options.length === options.length &&
      existing.options.every((option, index) => option === options[index]);
    if (!same) {
      // A new question, or one whose text changed: start from zero.
      if (existing) {
        const stale = await this.ctx.storage.list({ prefix: `vote:${id}:` });
        await this.ctx.storage.delete([...stale.keys()]);
      }
      await this.ctx.storage.put(`poll:${id}`, {
        question,
        options,
        counts: options.map(() => 0),
      });
    }
    await this.ctx.storage.put("current", id);
    await this.touch();
    return { status: 200, body: { id } };
  }

  async close(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    await this.ctx.storage.delete("current");
    await this.touch();
    return { status: 200, body: { ok: true } };
  }

  async results(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const current = await this.ctx.storage.get("current");
    const polls = {};
    for (const [name, poll] of await this.ctx.storage.list({ prefix: "poll:" })) {
      const id = name.slice("poll:".length);
      polls[id] = {
        open: id === current,
        counts: poll.counts,
        total: poll.counts.reduce((sum, count) => sum + count, 0),
      };
    }
    return { status: 200, body: { current: current ?? null, polls } };
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

async function votePage(env, url) {
  const page = await env.ASSETS.fetch(new URL("/vote", url.origin));
  const response = new Response(page.body, page);
  response.headers.set("cache-control", "no-store");
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) {
    response.headers.set(name, value);
  }
  return response;
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
  if (raw.length > MAX_BODY) throw new Error("request too large");
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
    headers: {
      "content-type": "application/json; charset=utf-8",
      "cache-control": "no-store",
      ...SECURITY_HEADERS,
    },
  });
}
