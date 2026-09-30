// Gaanim relay.
//
// A presentation opens a session under a six-character code and holds a
// secret key for it. Each poll has an id the presentation chooses, so a poll
// keeps its votes when the presentation comes back to it. The audience scans
// a QR code to /s/<code>, a page that shows the current question and sends
// one vote per phone. The presentation opens and closes questions and reads
// the counts with its key.
//
// A poll with a correct answer is a quiz, as in Kahoot: phones join with a
// nickname, have `time` seconds by the relay's clock to answer once, and
// earn up to `points` for a correct answer, more the faster they are. The
// presentation reveals the answer when it chooses; each phone then learns
// whether it was right, what it earned and its place on the leaderboard.
//
// Phones hold one WebSocket to the session: the relay pushes every change of
// question to all of them and takes their votes on it. A connection costs
// one request, incoming messages are billed 20 to 1 and outgoing ones are
// free, and an idle session hibernates, so a full room fits in the Workers
// Free plan. Plain HTTP stays for networks that block WebSockets.
//
//   GET    /                  join page: type a code (static asset)
//   GET    /s/<code>          voting page (public)
//   GET    /s/<code>/ws       WebSocket for phones (public), see below
//   GET    /s/<code>/poll     current question, without counts (public)
//   POST   /s/<code>/vote     {poll, option, voter} (public)
//   POST   /s/<code>/join     {voter, name} -> {player} (public)
//   GET    /s/<code>/player?voter=<id>  {player} (public)
//   PUT    /s/<code>/poll     {id, question, options, correct?, time?, points?} (presenter)
//   DELETE /s/<code>/poll     close the open question       (presenter)
//   POST   /s/<code>/reveal   {id}: show a quiz's answer    (presenter)
//   POST   /s/<code>/kick     {name}: remove and ban a player (presenter)
//   POST   /s/<code>/reset    forget every poll, vote and player (presenter)
//   GET    /s/<code>/results  {current, connected, polls, players} (presenter)
//   GET    /health            {relay, version}
//
// WebSocket messages are JSON, except the keepalive "ping", answered "pong"
// without waking the session. The relay sends {type: "poll", open, id,
// question, options, quiz?} on connect and whenever the question changes.
// A phone introduces itself with {type: "hello", voter} and gets {type:
// "player", player}; joins with {type: "join", voter, name}; and votes with
// {type: "vote", poll, option, voter}, answered {type: "voted", poll,
// option} or {type: "error", status, error, poll?}. When a quiz is revealed
// each phone gets {type: "result", poll, correct, option, points, player}.
// A removed player gets {type: "kicked"}.
//
// Votes are anonymous: a voter is a random id the page keeps in the phone's
// storage, and a nickname is all a quiz asks. A session and everything in it
// is deleted after SESSION_TTL_MS without activity.

import { DurableObject } from "cloudflare:workers";

const CODE = /^[A-HJ-NP-Z2-9]{6}$/;
const VOTER = /^[0-9a-f]{32}$/;
const KEY = /^[0-9a-f]{64}$/;
/** Poll ids are chosen by the presentation: stable across presentations. */
const POLL_ID = /^[a-z0-9][a-z0-9-]{0,63}$/;
/** Letters, digits, spaces and a little punctuation, in any script. */
const NAME = /^[\p{L}\p{N}][\p{L}\p{N} ._'-]*$/u;
const MIN_NAME = 2;
const MAX_NAME = 20;
const MAX_OPTIONS = 6;
const MAX_QUESTION = 300;
const MAX_OPTION = 120;
const MAX_BODY = 8192;
const MIN_TIME = 5;
const MAX_TIME = 300;
const DEFAULT_TIME = 20;
const MIN_POINTS = 100;
const MAX_POINTS = 10000;
const DEFAULT_POINTS = 1000;
/** Network slack for an answer sent right at the end of the time. */
const GRACE_MS = 500;
/** Players listed in the presenter's results. */
const LEADERBOARD = 100;
const SESSION_TTL_MS = 12 * 60 * 60 * 1000;
const API_VERSION = 4;

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
    if (parts[2] === "ws") {
      if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
        return json({ error: "expected a WebSocket upgrade" }, 426);
      }
      return session.fetch(request);
    }
    try {
      switch (`${request.method} ${parts[2]}`) {
        case "GET poll":
          return json(await session.current());
        case "POST vote":
          return reply(await session.vote(await body(request)));
        case "POST join":
          return reply(await session.join(await body(request)));
        case "GET player":
          return reply(await session.player(url.searchParams.get("voter")));
        case "PUT poll":
          return reply(await session.open(bearer(request), await body(request)));
        case "DELETE poll":
          return reply(await session.close(bearer(request)));
        case "POST reveal":
          return reply(await session.reveal(bearer(request), await body(request)));
        case "POST kick":
          return reply(await session.kick(bearer(request), await body(request)));
        case "POST reset":
          return reply(await session.reset(bearer(request)));
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

const ok = (body) => ({ status: 200, body });
const fail = (status, error) => ({ status, body: { error } });

export class PollSession extends DurableObject {
  // Storage:
  //   "key"                  presenter key hash
  //   "current"              id of the open poll, absent when none is
  //   "poll:<id>"            {question, options, counts, quiz?}; a quiz is
  //                          {correct, time, points, openedAt, deadline, revealed}
  //   "vote:<id>:<voter>"    answer index of a poll's voter
  //   "answer:<id>:<voter>"  {option, elapsed, points} of a quiz's player
  //   "player:<voter>"       {name, score, correct, answered, last}
  //   "name:<lowercase>"     voter holding a nickname
  //   "banned:<voter>"       a phone the presenter removed
  // Every poll keeps its votes, so a presentation that comes back to a
  // question finds them again.

  constructor(ctx, env) {
    super(ctx, env);
    // Keepalives are answered by the runtime: they never wake the session.
    this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
  }

  // --- Phones --------------------------------------------------------------

  // A phone's WebSocket. Accepted through the hibernation API, so the
  // session sleeps between messages while its sockets stay open.
  async fetch() {
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server);
    server.send(JSON.stringify({ type: "poll", ...(await this.current()) }));
    return new Response(null, { status: 101, webSocket: client });
  }

  async webSocketMessage(socket, message) {
    const send = (value) => socket.send(JSON.stringify(value));
    let input;
    try {
      if (typeof message !== "string" || message.length > MAX_BODY) throw new Error();
      input = JSON.parse(message);
    } catch {
      return send({ type: "error", status: 400, error: "invalid message" });
    }
    const remember = (voter) => {
      if (typeof voter === "string" && VOTER.test(voter)) socket.serializeAttachment({ voter });
    };
    switch (input?.type) {
      case "hello": {
        remember(input.voter);
        const result = await this.player(input.voter);
        return send(
          result.status === 200
            ? { type: "player", player: result.body.player }
            : { type: "error", status: result.status, error: result.body.error },
        );
      }
      case "join": {
        const result = await this.join(input);
        if (result.status !== 200) {
          return send({ type: "error", status: result.status, error: result.body.error });
        }
        remember(input.voter);
        return send({ type: "joined", player: result.body.player });
      }
      case "vote": {
        remember(input.voter);
        const result = await this.vote(input);
        return send(
          result.status === 200
            ? { type: "voted", poll: input.poll, option: input.option }
            : { type: "error", status: result.status, error: result.body.error, poll: input.poll },
        );
      }
      default:
        return send({ type: "error", status: 400, error: "unknown message" });
    }
  }

  async webSocketClose(socket, code, reason) {
    try {
      socket.close(code, reason);
    } catch {
      // Already closed, or a code that cannot be sent back (1005, 1006).
    }
  }

  async webSocketError() {}

  /** Send `message(voter)` to every phone, or skip a phone when it is null. */
  send(message) {
    for (const socket of this.ctx.getWebSockets()) {
      const voter = socket.deserializeAttachment()?.voter ?? null;
      const value = message(voter);
      if (value === null) continue;
      try {
        socket.send(JSON.stringify(value));
      } catch {
        // A socket closing right now; its phone reconnects and catches up.
      }
    }
  }

  /** Tell every connected phone what the question is now. */
  async broadcast() {
    const state = { type: "poll", ...(await this.current()) };
    this.send(() => state);
  }

  async current() {
    const id = await this.ctx.storage.get("current");
    const poll = id && (await this.ctx.storage.get(`poll:${id}`));
    if (!poll) {
      return { open: false };
    }
    const state = { open: true, id, question: poll.question, options: poll.options };
    if (poll.quiz) {
      // The deadline is on the relay's clock; `now` lets a phone correct
      // for the difference with its own.
      state.quiz = {
        time: poll.quiz.time,
        deadline: poll.quiz.deadline,
        now: Date.now(),
        revealed: poll.quiz.revealed,
      };
      // The answer is secret until the presenter reveals it.
      if (poll.quiz.revealed) state.quiz.correct = poll.quiz.correct;
    }
    return state;
  }

  async player(voter) {
    if (typeof voter !== "string" || !VOTER.test(voter)) {
      return fail(400, "invalid voter");
    }
    if (await this.ctx.storage.get(`banned:${voter}`)) {
      return fail(403, "removed by the presenter");
    }
    const player = await this.ctx.storage.get(`player:${voter}`);
    return ok({ player: player ? await this.describe(voter, player) : null });
  }

  /** A player as the phone sees it: with its place among all players. */
  async describe(voter, player, ranking = null) {
    const players = ranking ?? (await this.ranking());
    const rank = players.findIndex((entry) => entry.voter === voter) + 1;
    return {
      name: player.name,
      score: player.score,
      correct: player.correct,
      answered: player.answered,
      rank: rank || null,
      players: players.length,
      last: player.last ?? null,
    };
  }

  /** Every player, best first; ties go to the most correct answers. */
  async ranking() {
    const players = [];
    for (const [key, player] of await this.ctx.storage.list({ prefix: "player:" })) {
      players.push({ voter: key.slice("player:".length), ...player });
    }
    players.sort(
      (a, b) => b.score - a.score || b.correct - a.correct || a.name.localeCompare(b.name),
    );
    return players;
  }

  async join(input) {
    const voter = input?.voter;
    if (typeof voter !== "string" || !VOTER.test(voter)) {
      return fail(400, "invalid voter");
    }
    if (await this.ctx.storage.get(`banned:${voter}`)) {
      return fail(403, "removed by the presenter");
    }
    const name = typeof input.name === "string" ? input.name.trim().replace(/\s+/g, " ") : "";
    if ([...name].length < MIN_NAME || [...name].length > MAX_NAME || !NAME.test(name)) {
      return fail(400, "invalid name");
    }
    const lower = name.toLocaleLowerCase();
    const holder = await this.ctx.storage.get(`name:${lower}`);
    if (holder && holder !== voter) {
      return fail(409, "name taken");
    }
    const player = (await this.ctx.storage.get(`player:${voter}`)) ?? {
      name,
      score: 0,
      correct: 0,
      answered: 0,
      last: null,
    };
    if (player.name.toLocaleLowerCase() !== lower) {
      await this.ctx.storage.delete(`name:${player.name.toLocaleLowerCase()}`);
    }
    player.name = name;
    await this.ctx.storage.put({ [`player:${voter}`]: player, [`name:${lower}`]: voter });
    await this.touch();
    return ok({ player: await this.describe(voter, player) });
  }

  async vote(input) {
    const id = await this.ctx.storage.get("current");
    if (!id || input?.poll !== id) {
      return fail(409, "this question is closed");
    }
    const poll = await this.ctx.storage.get(`poll:${id}`);
    const option = input.option;
    if (!Number.isInteger(option) || option < 0 || option >= poll.options.length) {
      return fail(400, "unknown answer");
    }
    const voter = input.voter;
    if (typeof voter !== "string" || !VOTER.test(voter)) {
      return fail(400, "invalid voter");
    }
    if (poll.quiz) {
      return this.answer(id, poll, voter, option);
    }
    const key = `vote:${id}:${voter}`;
    const previous = await this.ctx.storage.get(key);
    if (previous === option) {
      return ok({ ok: true });
    }
    if (previous !== undefined) {
      poll.counts[previous] = Math.max(0, poll.counts[previous] - 1);
    }
    poll.counts[option] += 1;
    await this.ctx.storage.put({ [key]: option, [`poll:${id}`]: poll });
    await this.touch();
    return ok({ ok: true });
  }

  /** A quiz answer: once per player, before the deadline, scored by speed. */
  async answer(id, poll, voter, option) {
    const quiz = poll.quiz;
    const now = Date.now();
    if (quiz.revealed || now > quiz.deadline + GRACE_MS) {
      return fail(409, "time is up");
    }
    if (await this.ctx.storage.get(`banned:${voter}`)) {
      return fail(403, "removed by the presenter");
    }
    const player = await this.ctx.storage.get(`player:${voter}`);
    if (!player) {
      return fail(401, "join with a name first");
    }
    const key = `answer:${id}:${voter}`;
    if (await this.ctx.storage.get(key)) {
      return fail(409, "already answered");
    }
    const limit = quiz.time * 1000;
    const elapsed = Math.min(Math.max(now - quiz.openedAt, 0), limit);
    const points = option === quiz.correct ? Math.round(quiz.points * (1 - elapsed / limit / 2)) : 0;
    poll.counts[option] += 1;
    player.answered += 1;
    // The score changes now, but phones learn it only when the answer is
    // revealed; the leaderboard is the presenter's to show.
    player.score += points;
    if (points > 0) player.correct += 1;
    await this.ctx.storage.put({
      [key]: { option, elapsed, points },
      [`poll:${id}`]: poll,
      [`player:${voter}`]: player,
    });
    await this.touch();
    return ok({ ok: true });
  }

  // --- Presenter -----------------------------------------------------------

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
      return fail(400, "invalid question");
    }
    let quiz = null;
    if (input.correct !== undefined && input.correct !== null) {
      const time = input.time ?? DEFAULT_TIME;
      const points = input.points ?? DEFAULT_POINTS;
      if (
        !Number.isInteger(input.correct) ||
        input.correct < 0 ||
        input.correct >= options.length ||
        !Number.isInteger(time) ||
        time < MIN_TIME ||
        time > MAX_TIME ||
        !Number.isInteger(points) ||
        points < MIN_POINTS ||
        points > MAX_POINTS
      ) {
        return fail(400, "invalid quiz");
      }
      quiz = { correct: input.correct, time, points };
    }
    const existing = await this.ctx.storage.get(`poll:${id}`);
    const same =
      existing &&
      existing.question === question &&
      existing.options.length === options.length &&
      existing.options.every((option, index) => option === options[index]) &&
      (existing.quiz?.correct ?? null) === (quiz?.correct ?? null) &&
      (existing.quiz?.time ?? null) === (quiz?.time ?? null) &&
      (existing.quiz?.points ?? null) === (quiz?.points ?? null);
    if (!same) {
      // A new question, or one whose text or scoring changed: start from zero.
      if (existing) await this.forget(id);
      const now = Date.now();
      await this.ctx.storage.put(`poll:${id}`, {
        question,
        options,
        counts: options.map(() => 0),
        quiz: quiz && { ...quiz, openedAt: now, deadline: now + quiz.time * 1000, revealed: false },
      });
    }
    // Coming back to a quiz keeps its clock: its time may already be up.
    const previous = await this.ctx.storage.get("current");
    await this.ctx.storage.put("current", id);
    await this.touch();
    if (previous !== id || !same) {
      await this.broadcast();
    }
    return ok({ id });
  }

  /** Drop a poll's votes and answers, and the points its answers gave. */
  async forget(id) {
    const votes = await this.ctx.storage.list({ prefix: `vote:${id}:` });
    const answers = await this.ctx.storage.list({ prefix: `answer:${id}:` });
    for (const [key, answer] of answers) {
      const voter = key.slice(`answer:${id}:`.length);
      const player = await this.ctx.storage.get(`player:${voter}`);
      if (player) {
        player.score -= answer.points;
        player.answered -= 1;
        if (answer.points > 0) player.correct -= 1;
        await this.ctx.storage.put(`player:${voter}`, player);
      }
    }
    await this.ctx.storage.delete([...votes.keys(), ...answers.keys()]);
  }

  async close(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const open = await this.ctx.storage.delete("current");
    await this.touch();
    if (open) {
      await this.broadcast();
    }
    return ok({ ok: true });
  }

  /** Show a quiz's answer: it takes no more answers, and each phone learns
   * how it did. */
  async reveal(key, input) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const id = input?.id;
    const poll = typeof id === "string" && (await this.ctx.storage.get(`poll:${id}`));
    if (!poll?.quiz) {
      return fail(404, "no such quiz");
    }
    poll.quiz.revealed = true;
    await this.ctx.storage.put(`poll:${id}`, poll);
    const answers = await this.ctx.storage.list({ prefix: `answer:${id}:` });
    const results = new Map();
    for (const [key, player] of await this.ctx.storage.list({ prefix: "player:" })) {
      const voter = key.slice("player:".length);
      const answer = answers.get(`answer:${id}:${voter}`);
      player.last = {
        poll: id,
        correct: poll.quiz.correct,
        option: answer?.option ?? null,
        points: answer?.points ?? 0,
      };
      await this.ctx.storage.put(`player:${voter}`, player);
      results.set(voter, player);
    }
    // Every phone learns the answer; players also learn their result.
    const ranking = await this.ranking();
    const outcomes = new Map();
    for (const [voter, player] of results) {
      outcomes.set(voter, await this.describe(voter, player, ranking));
    }
    this.send((voter) => {
      const player = voter && outcomes.get(voter);
      return {
        type: "result",
        poll: id,
        correct: poll.quiz.correct,
        option: player?.last.option ?? null,
        points: player?.last.points ?? 0,
        player: player ?? null,
      };
    });
    await this.touch();
    return ok({ ok: true });
  }

  /** Remove a player, ban its phone from the session and free its name. */
  async kick(key, input) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const name = typeof input?.name === "string" ? input.name.trim().toLocaleLowerCase() : "";
    const voter = name && (await this.ctx.storage.get(`name:${name}`));
    if (!voter) {
      return fail(404, "no such player");
    }
    await this.ctx.storage.delete([`player:${voter}`, `name:${name}`]);
    await this.ctx.storage.put(`banned:${voter}`, true);
    for (const socket of this.ctx.getWebSockets()) {
      if (socket.deserializeAttachment()?.voter !== voter) continue;
      try {
        socket.send(JSON.stringify({ type: "kicked" }));
        socket.close(4003, "removed by the presenter");
      } catch {
        // Already gone.
      }
    }
    await this.touch();
    return ok({ ok: true });
  }

  /** Start over: no polls, votes, players or bans; the key stays. */
  async reset(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const stored = await this.ctx.storage.get("key");
    await this.ctx.storage.deleteAll();
    await this.ctx.storage.put("key", stored);
    await this.touch();
    await this.broadcast();
    this.send(() => ({ type: "player", player: null }));
    return ok({ ok: true });
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
      if (poll.quiz) {
        polls[id].quiz = {
          correct: poll.quiz.correct,
          deadline: poll.quiz.deadline,
          revealed: poll.quiz.revealed,
        };
      }
    }
    const ranking = await this.ranking();
    const players = ranking.slice(0, LEADERBOARD).map((player) => ({
      name: player.name,
      score: player.score,
      correct: player.correct,
      answered: player.answered,
    }));
    const connected = this.ctx.getWebSockets().length;
    return ok({
      current: current ?? null,
      connected,
      now: Date.now(),
      polls,
      players,
      playerCount: ranking.length,
    });
  }

  // The first presenter request claims the session with its key.
  async authorize(key, claim) {
    if (typeof key !== "string" || !KEY.test(key)) {
      return fail(401, "missing presenter key");
    }
    const hash = await sha256(key);
    const stored = await this.ctx.storage.get("key");
    if (!stored) {
      if (!claim) return fail(401, "unknown session");
      await this.ctx.storage.put("key", hash);
      return null;
    }
    return stored === hash ? null : fail(403, "wrong presenter key");
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
  // Older Safari does not count this host's WebSocket as 'self'.
  const socket = `${url.protocol === "https:" ? "wss" : "ws"}://${url.host}`;
  response.headers.set(
    "content-security-policy",
    (page.headers.get("content-security-policy") ?? "default-src 'self'").replace(
      "connect-src 'self'",
      `connect-src 'self' ${socket}`,
    ),
  );
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
