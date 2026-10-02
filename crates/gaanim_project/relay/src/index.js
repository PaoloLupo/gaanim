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
//   GET    /s/<code>/poll?voter=<id>  current question, without counts (public)
//   POST   /s/<code>/vote     {poll, option, voter} (public)
//   POST   /s/<code>/join     {voter, name} -> {player} (public)
//   GET    /s/<code>/player?voter=<id>  {player} (public)
//   PUT    /s/<code>/poll     {id, question, options, correct?, time?, points?} (presenter)
//   DELETE /s/<code>/poll     close the open question       (presenter)
//   POST   /s/<code>/reveal   {id}: show a quiz's answer    (presenter)
//   POST   /s/<code>/kick     {name}: remove and ban a player (presenter)
//   POST   /s/<code>/reset    forget every poll, vote and player (presenter)
//   POST   /s/<code>/lobby    {open}: phones join as they arrive (presenter)
//   POST   /s/<code>/teams    {names, colors, choose}: play in teams (presenter)
//   PUT    /s/<code>/image/<hash>  a poll's picture, its bytes (presenter)
//   GET    /s/<code>/image/<hash>  that picture (public)
//   GET    /s/<code>/results  {current, connected, polls, players, audience} (presenter)
//   GET    /s/<code>/presenter  WebSocket pushing {type: "results", ...} (presenter)
//   GET    /health            {relay, version}
//
// WebSocket messages are JSON, except the keepalive "ping", answered "pong"
// without waking the session. The relay sends {type: "poll", lobby, open,
// id, question, options, quiz?, chosen} on connect and whenever the question
// changes; `chosen` is the phone's own vote or answer (null for none), so a
// phone never trusts a vote it remembers from an earlier game, and `lobby`
// asks a phone that has not joined for a nickname right away. A phone
// introduces itself with {type: "hello", voter} and gets {type: "player",
// player} and the question with its `chosen`; joins with {type: "join", voter, name}; and votes with
// {type: "vote", poll, option, voter}, answered {type: "voted", poll,
// option} or {type: "error", status, error, poll?}. When a quiz is revealed
// each phone gets {type: "result", poll, correct, option, points, player}.
// A removed player gets {type: "kicked"}. A join may carry the player's
// character, `avatar`: [body, color, eyes, mouth, extra], indexes into
// public/avatar-parts.json; players and the audience carry it back.
//
// The presentation holds its own WebSocket, opened with its key: the relay
// sends it the results on connect and again whenever they change, at most
// every PUSH_MS, so it never asks. It only sends keepalives.
//
// Votes are anonymous: a voter is a random id the page keeps in the phone's
// storage, and a nickname is all a quiz asks. A session and everything in it
// is deleted after SESSION_TTL_MS without activity.

import { DurableObject } from "cloudflare:workers";
// The characters' parts, the same file the voting page draws from.
import avatarParts from "../public/avatar-parts.json";

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
/** Players listed in joining order in the presenter's results. */
const AUDIENCE = 200;
const SESSION_TTL_MS = 12 * 60 * 60 * 1000;
/** How far behind the alarm may fall before activity moves it: a session
 * lasts between SESSION_TTL_MS minus this and SESSION_TTL_MS after its last
 * activity. */
const ALARM_SLACK_MS = 30 * 60 * 1000;
const API_VERSION = 11;
/** A poll picture's name: a hash of its bytes. */
const IMAGE = /^[0-9a-f]{16}$/;
/** Largest picture the presenter may store. */
const MAX_IMAGE = 600_000;
const IMAGE_TYPES = new Set(["image/jpeg", "image/png", "image/webp"]);
/** Teams a game may have, and the longest team name. */
const MAX_TEAMS = 6;
const MAX_TEAM_NAME = 20;
const COLOR = /^#[0-9a-f]{6}$/;
/** Where the game is: questions, the final standings, or over. */
const STAGES = new Set(["play", "podium", "end"]);
/** Players a podium shows. */
const PODIUM = 3;
/** How long changes gather before the presenter's socket hears of them, so
 * a burst of votes is one message. */
const PUSH_MS = 250;
/** Players a game takes, and phones a poll counts: anyone with the code can
 * join, so a script cannot grow a session without end. */
const MAX_PLAYERS = 500;
const MAX_VOTERS = 1000;
/** Messages a phone's socket may send per RATE_WINDOW_MS; a phone sends a
 * few per question. */
const RATE_MESSAGES = 20;
const RATE_WINDOW_MS = 10_000;

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
    const picture = parts[2] === "image" && parts.length === 4;
    if (parts[0] !== "s" || !CODE.test(code) || (parts.length > 3 && !picture)) {
      return json({ error: "not found" }, 404);
    }
    if (parts.length === 2) {
      if (request.method !== "GET") return json({ error: "method not allowed" }, 405);
      // Short, shareable address: /s/abc234 → /s/ABC234.
      if (parts[1] !== code) return Response.redirect(`${url.origin}/s/${code}`, 302);
      return votePage(env, url);
    }
    const session = env.SESSIONS.get(env.SESSIONS.idFromName(code));
    if (parts[2] === "ws" || parts[2] === "presenter") {
      if (request.headers.get("upgrade")?.toLowerCase() !== "websocket") {
        return json({ error: "expected a WebSocket upgrade" }, 426);
      }
      return session.fetch(request);
    }
    if (picture) {
      const hash = parts[3];
      if (!IMAGE.test(hash)) return json({ error: "not found" }, 404);
      if (request.method === "GET") {
        const image = await session.image(hash);
        if (!image) return json({ error: "not found" }, 404);
        return new Response(image.bytes, {
          headers: {
            "content-type": image.mime,
            // Named by its bytes: it never changes.
            "cache-control": "public, max-age=31536000, immutable",
          },
        });
      }
      if (request.method === "PUT") {
        const mime = (request.headers.get("content-type") ?? "").split(";")[0].trim();
        if (!IMAGE_TYPES.has(mime) || Number(request.headers.get("content-length") ?? 0) > MAX_IMAGE) {
          return json({ error: "invalid image" }, 400);
        }
        const bytes = await request.arrayBuffer();
        if (bytes.byteLength === 0 || bytes.byteLength > MAX_IMAGE) {
          return json({ error: "invalid image" }, 400);
        }
        return reply(await session.store(bearer(request), hash, mime, bytes));
      }
      return json({ error: "method not allowed" }, 405);
    }
    try {
      switch (`${request.method} ${parts[2]}`) {
        case "GET poll": {
          const voter = url.searchParams.get("voter");
          return json(await session.current(VOTER.test(voter ?? "") ? voter : null));
        }
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
        case "POST lobby":
          return reply(await session.lobby(bearer(request), await body(request)));
        case "POST teams":
          return reply(await session.teams(bearer(request), await body(request)));
        case "POST stage":
          return reply(await session.stage(bearer(request), await body(request)));
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
  //   "lobby"                true when phones join as soon as they arrive
  //   "stage"                "podium" or "end" once the questions are over
  //   "teams"                {names, colors, choose} when the game plays in teams
  //   "current"              id of the open poll, absent when none is
  //   "revealed"             id of the quiz revealed last
  //   "latest"               id of the poll opened last
  //   "game"                 id of the game, new with each one
  //   "poll:<id>"            {question, options, multiple?, image?, quiz?}; a quiz is
  //                          {correct, time, points, openedAt, deadline, revealed};
  //                          `correct` is an index, or a list for multiple choice
  //   "vote:<id>:<voter>"    a poll voter's answer: an index, or a list
  //   "answer:<id>:<voter>"  {option, elapsed, points} of a quiz's player
  //   "player:<voter>"       {name, score, correct, answered, streak, joined, avatar}
  //   "image:<hash>"         {mime, bytes} of a poll picture; kept across games
  //   "banned:<voter>"       a phone the presenter removed
  // Every poll keeps its votes, so a presentation that comes back to a
  // question finds them again.
  //
  // Storage is billed by the row, and the presenter reads the results every
  // second, so the session reads all of it once when it wakes and answers
  // from memory: vote counts, nicknames and the ranking are derived there.
  // Each change is stored first and then applied to memory, so a session
  // evicted at any moment wakes to the same state.

  constructor(ctx, env) {
    super(ctx, env);
    // Keepalives are answered by the runtime: they never wake the session.
    this.ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair("ping", "pong"));
    /** The session in memory, loaded on first use. */
    this.loading = null;
    /** The presenter key last checked, so its hash is computed once. */
    this.verified = null;
    /** A push of the results to the presentation, while one is due. */
    this.pushTimer = null;
  }

  /** The session's state, read from storage once per wake. */
  state() {
    this.loading ??= this.load().catch((error) => {
      this.loading = null;
      throw error;
    });
    return this.loading;
  }

  async load() {
    const s = {
      key: null,
      lobby: false,
      teams: null,
      stage: "play",
      current: null,
      latest: null,
      game: null,
      revealed: null,
      polls: new Map(),
      votes: new Map(),
      answers: new Map(),
      players: new Map(),
      names: new Map(),
      banned: new Set(),
      alarmAt: await this.ctx.storage.getAlarm(),
    };
    const byPoll = (map, id) => map.get(id) ?? map.set(id, new Map()).get(id);
    // Everything but the pictures, which are large and read when a phone
    // asks for one: the keys before "image:" and from "image;" on.
    const rows = [
      ...(await this.ctx.storage.list({ end: "image:" })),
      ...(await this.ctx.storage.list({ start: "image;" })),
    ];
    for (const [key, value] of rows) {
      const [kind, id, voter] = key.split(":");
      switch (kind) {
        case "key":
          s.key = value;
          break;
        case "lobby":
          s.lobby = value === true;
          break;
        case "stage":
          if (STAGES.has(value)) s.stage = value;
          break;
        case "teams":
          s.teams = value;
          break;
        case "current":
          s.current = value;
          break;
        case "revealed":
          s.revealed = value;
          break;
        case "latest":
          s.latest = value;
          break;
        case "game":
          s.game = value;
          break;
        case "poll":
          s.polls.set(id, {
            question: value.question,
            options: value.options,
            multiple: value.multiple === true,
            image: value.image ?? null,
            quiz: value.quiz ?? null,
          });
          break;
        case "vote":
          byPoll(s.votes, id).set(voter, value);
          break;
        case "answer":
          byPoll(s.answers, id).set(voter, value);
          break;
        case "player":
          s.players.set(id, value);
          s.names.set(value.name.toLocaleLowerCase(), id);
          break;
        case "banned":
          s.banned.add(id);
          break;
      }
    }
    for (const [id, poll] of s.polls) poll.counts = countVotes(s, id, poll);
    return s;
  }

  // --- Phones --------------------------------------------------------------

  // A phone's WebSocket. Accepted through the hibernation API, so the
  // session sleeps between messages while its sockets stay open.
  // Phones' sockets are tagged "phone", the presentation's "presenter".
  async fetch(request) {
    const presenter = new URL(request.url).pathname.endsWith("/presenter");
    if (presenter) {
      const denied = await this.authorize(bearer(request), true);
      if (denied) return json(denied.body, denied.status);
    }
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server, [presenter ? "presenter" : "phone"]);
    if (presenter) {
      server.send(JSON.stringify({ type: "results", ...(await this.summary(true)) }));
    } else {
      server.send(JSON.stringify({ type: "poll", ...(await this.current()) }));
      // One more phone connected.
      this.schedulePush();
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  /** Tell the presentation the results soon: changes within PUSH_MS go
   * out together. */
  schedulePush() {
    if (this.pushTimer || this.ctx.getWebSockets("presenter").length === 0) return;
    this.pushTimer = setTimeout(async () => {
      this.pushTimer = null;
      const sockets = this.ctx.getWebSockets("presenter");
      if (sockets.length === 0) return;
      const message = JSON.stringify({ type: "results", ...(await this.summary()) });
      for (const socket of sockets) {
        try {
          socket.send(message);
        } catch {
          // Closing; the presentation reconnects and gets the results then.
        }
      }
    }, PUSH_MS);
  }

  async webSocketMessage(socket, message) {
    // The presentation's socket only listens.
    if (this.ctx.getTags(socket).includes("presenter")) return;
    const send = (value) => socket.send(JSON.stringify(value));
    // The socket's phone and its recent messages, kept with the socket so
    // they survive hibernation.
    const attachment = socket.deserializeAttachment() ?? {};
    const now = Date.now();
    const fresh = !(now - attachment.since < RATE_WINDOW_MS);
    const since = fresh ? now : attachment.since;
    const count = fresh ? 1 : (attachment.count ?? 0) + 1;
    socket.serializeAttachment({ ...attachment, since, count });
    if (count > 3 * RATE_MESSAGES) return socket.close(1008, "too many messages");
    let input;
    let invalid = false;
    try {
      if (typeof message !== "string" || message.length > MAX_BODY) throw new Error();
      input = JSON.parse(message);
    } catch {
      invalid = true;
    }
    const poll = input?.type === "vote" ? { poll: input.poll } : {};
    if (count > RATE_MESSAGES) {
      return send({ type: "error", status: 429, error: "too many messages", ...poll });
    }
    if (invalid) return send({ type: "error", status: 400, error: "invalid message" });
    // A socket speaks for one phone: the first voter id it gives.
    if (attachment.voter && input?.voter !== undefined && input.voter !== attachment.voter) {
      return send({ type: "error", status: 400, error: "another phone's voter", ...poll });
    }
    const remember = (voter) => {
      if (!attachment.voter && typeof voter === "string" && VOTER.test(voter)) {
        socket.serializeAttachment({ voter, since, count });
      }
    };
    switch (input?.type) {
      case "hello": {
        remember(input.voter);
        const result = await this.player(input.voter);
        if (result.status !== 200) {
          return send({ type: "error", status: result.status, error: result.body.error });
        }
        send({ type: "player", player: result.body.player });
        // The question again, now with this phone's own vote.
        return send({ type: "poll", ...(await this.current(input.voter)) });
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
    // One phone fewer.
    if (this.ctx.getTags(socket).includes("phone")) this.schedulePush();
  }

  async webSocketError() {}

  /** Send `message(voter)` to every phone, or skip a phone when it is null. */
  send(message) {
    for (const socket of this.ctx.getWebSockets("phone")) {
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

  /** Tell every connected phone what the question is now, and what it
   * chose there. */
  async broadcast() {
    const s = await this.state();
    const state = { type: "poll", ...(await this.current()) };
    this.send((voter) => ({ ...state, chosen: choiceOf(s, state.id, voter) }));
  }

  /** The open question; with a `voter`, also what that phone chose. */
  async current(voter = null) {
    const s = await this.state();
    const id = s.current;
    const poll = id && s.polls.get(id);
    // A phone asking with its id learns whether it still plays: a new game
    // forgets every player.
    const joined = voter ? { joined: s.players.has(voter) } : {};
    const teams = s.teams ? { teams: s.teams } : {};
    if (!poll) {
      const state = { lobby: s.lobby, open: false, stage: s.stage, ...joined, ...teams };
      if (s.stage !== "play") {
        // The final standings, for phones to show their podium.
        const ranking = rank(s);
        state.players = ranking.length;
        state.podium = ranking.slice(0, PODIUM).map((player) => ({
          name: player.name,
          score: player.score,
          avatar: player.avatar ?? defaultAvatar(player.voter),
        }));
        if (s.teams) state.standings = teamStandings(s);
      }
      return state;
    }
    const state = {
      ...joined,
      ...teams,
      lobby: s.lobby,
      stage: s.stage,
      open: true,
      id,
      question: poll.question,
      options: poll.options,
      multiple: poll.multiple,
      image: poll.image,
      chosen: voter ? choiceOf(s, id, voter) : null,
    };
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
    const s = await this.state();
    if (s.banned.has(voter)) {
      return fail(403, "removed by the presenter");
    }
    const player = s.players.get(voter);
    return ok({ player: player ? describe(s, voter, player) : null });
  }

  async join(input) {
    const voter = input?.voter;
    if (typeof voter !== "string" || !VOTER.test(voter)) {
      return fail(400, "invalid voter");
    }
    const s = await this.state();
    if (s.banned.has(voter)) {
      return fail(403, "removed by the presenter");
    }
    const name = typeof input.name === "string" ? input.name.trim().replace(/\s+/g, " ") : "";
    if ([...name].length < MIN_NAME || [...name].length > MAX_NAME || !NAME.test(name)) {
      return fail(400, "invalid name");
    }
    const lower = name.toLocaleLowerCase();
    const holder = s.names.get(lower);
    if (holder && holder !== voter) {
      return fail(409, "name taken");
    }
    const previous = s.players.get(voter);
    if (!previous && s.players.size >= MAX_PLAYERS) {
      return fail(503, "the game is full");
    }
    const avatar = checkAvatar(input.avatar) ?? previous?.avatar ?? defaultAvatar(voter);
    // A team: kept once dealt; a chosen one may change until the player
    // answers a question.
    let team = previous?.team ?? null;
    if (s.teams) {
      const count = s.teams.names.length;
      const wanted = input.team;
      const valid = Number.isInteger(wanted) && wanted >= 0 && wanted < count;
      if (s.teams.choose) {
        if (valid && (team === null || !previous?.answered)) team = wanted;
        if (team === null) return fail(400, "choose a team");
      } else if (team === null || team >= count) {
        team = smallestTeam(s);
      }
    }
    if (
      previous?.name !== name ||
      String(previous?.avatar) !== String(avatar) ||
      (previous?.team ?? null) !== team
    ) {
      const player = previous
        ? { ...previous, name, avatar, team }
        : { name, score: 0, correct: 0, answered: 0, joined: Date.now(), avatar, team };
      await this.ctx.storage.put(`player:${voter}`, player);
      if (previous) s.names.delete(previous.name.toLocaleLowerCase());
      s.players.set(voter, player);
      s.names.set(lower, voter);
    }
    await this.touch(s);
    return ok({ player: describe(s, voter, s.players.get(voter)) });
  }

  async vote(input) {
    const s = await this.state();
    const id = s.current;
    if (!id || input?.poll !== id) {
      return fail(409, "this question is closed");
    }
    const poll = s.polls.get(id);
    const option = checkChoice(input.option, poll);
    if (option === null) {
      return fail(400, "unknown answer");
    }
    const voter = input.voter;
    if (typeof voter !== "string" || !VOTER.test(voter)) {
      return fail(400, "invalid voter");
    }
    if (poll.quiz) {
      return this.answer(s, id, poll, voter, option);
    }
    const votes = s.votes.get(id) ?? s.votes.set(id, new Map()).get(id);
    const previous = votes.get(voter);
    if (previous !== undefined && sameChoice(previous, option)) {
      return ok({ ok: true });
    }
    if (previous === undefined && votes.size >= MAX_VOTERS) {
      return fail(503, "the poll is full");
    }
    await this.ctx.storage.put(`vote:${id}:${voter}`, option);
    if (previous !== undefined) {
      for (const chosen of choices(previous)) poll.counts[chosen] = Math.max(0, poll.counts[chosen] - 1);
    }
    for (const chosen of choices(option)) poll.counts[chosen] += 1;
    votes.set(voter, option);
    await this.touch(s);
    return ok({ ok: true });
  }

  /** A quiz answer: once per player, before the deadline, scored by speed. */
  async answer(s, id, poll, voter, option) {
    const quiz = poll.quiz;
    const now = Date.now();
    if (quiz.revealed || now > quiz.deadline + GRACE_MS) {
      return fail(409, "time is up");
    }
    if (s.banned.has(voter)) {
      return fail(403, "removed by the presenter");
    }
    const player = s.players.get(voter);
    if (!player) {
      return fail(401, "join with a name first");
    }
    const answers = s.answers.get(id) ?? s.answers.set(id, new Map()).get(id);
    if (answers.has(voter)) {
      return fail(409, "already answered");
    }
    const limit = quiz.time * 1000;
    const elapsed = Math.min(Math.max(now - quiz.openedAt, 0), limit);
    // Right means every right answer and no other.
    const points = sameChoice(option, quiz.correct)
      ? Math.round(quiz.points * (1 - elapsed / limit / 2))
      : 0;
    // The score changes now, but phones learn it only when the answer is
    // revealed; the leaderboard is the presenter's to show.
    const scored = {
      ...player,
      score: player.score + points,
      correct: player.correct + (points > 0 ? 1 : 0),
      answered: player.answered + 1,
    };
    const answer = { option, elapsed, points };
    await this.ctx.storage.put({ [`answer:${id}:${voter}`]: answer, [`player:${voter}`]: scored });
    answers.set(voter, answer);
    s.players.set(voter, scored);
    for (const chosen of choices(option)) poll.counts[chosen] += 1;
    await this.touch(s);
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
    const multiple = input.multiple === true || Array.isArray(input.correct);
    const image = input.image ?? null;
    if (image !== null && (typeof image !== "string" || !IMAGE.test(image))) {
      return fail(400, "invalid image");
    }
    let quiz = null;
    if (input.correct !== undefined && input.correct !== null) {
      const time = input.time ?? DEFAULT_TIME;
      const points = input.points ?? DEFAULT_POINTS;
      const correct = checkChoice(input.correct, { options, multiple });
      if (
        correct === null ||
        !Number.isInteger(time) ||
        time < MIN_TIME ||
        time > MAX_TIME ||
        !Number.isInteger(points) ||
        points < MIN_POINTS ||
        points > MAX_POINTS
      ) {
        return fail(400, "invalid quiz");
      }
      quiz = { correct, time, points };
    }
    const s = await this.startOver(await this.state());
    const existing = s.polls.get(id);
    const same =
      existing &&
      existing.question === question &&
      existing.options.length === options.length &&
      existing.options.every((option, index) => option === options[index]) &&
      existing.multiple === multiple &&
      existing.image === image &&
      JSON.stringify(existing.quiz?.correct ?? null) === JSON.stringify(quiz?.correct ?? null) &&
      (existing.quiz?.time ?? null) === (quiz?.time ?? null) &&
      (existing.quiz?.points ?? null) === (quiz?.points ?? null);
    if (!same) {
      // A new question, or one whose text or scoring changed: start from zero.
      if (existing) await this.forget(s, id);
      const now = Date.now();
      const poll = {
        question,
        options,
        multiple,
        image,
        quiz: quiz && { ...quiz, openedAt: now, deadline: now + quiz.time * 1000, revealed: false },
      };
      await this.ctx.storage.put(`poll:${id}`, poll);
      s.polls.set(id, { ...poll, counts: options.map(() => 0) });
    }
    // Coming back to a quiz keeps its clock: its time may already be up.
    const previous = s.current;
    if (previous !== id) {
      await this.ctx.storage.put({ current: id, latest: id });
      s.current = id;
      s.latest = id;
    }
    await this.touch(s);
    if (previous !== id || !same) {
      await this.broadcast();
    }
    return ok({ id });
  }

  /** Drop a poll's votes and answers, and the points its answers gave. */
  async forget(s, id) {
    const answers = s.answers.get(id) ?? new Map();
    const players = {};
    for (const [voter, answer] of answers) {
      const player = s.players.get(voter);
      if (!player) continue;
      players[`player:${voter}`] = {
        ...player,
        score: player.score - answer.points,
        answered: player.answered - 1,
        correct: player.correct - (answer.points > 0 ? 1 : 0),
      };
    }
    await putAll(this.ctx.storage, players);
    const keys = [
      ...[...(s.votes.get(id)?.keys() ?? [])].map((voter) => `vote:${id}:${voter}`),
      ...[...answers.keys()].map((voter) => `answer:${id}:${voter}`),
    ];
    await deleteAll(this.ctx.storage, keys);
    for (const [key, player] of Object.entries(players)) {
      s.players.set(key.slice("player:".length), player);
    }
    s.votes.delete(id);
    s.answers.delete(id);
  }

  async close(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const s = await this.state();
    const open = s.current !== null;
    if (open) {
      await this.ctx.storage.delete("current");
      s.current = null;
    }
    await this.touch(s);
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
    const s = await this.state();
    const id = input?.id;
    const poll = typeof id === "string" && s.polls.get(id);
    if (!poll?.quiz) {
      return fail(404, "no such quiz");
    }
    const first = !poll.quiz.revealed;
    const quiz = { ...poll.quiz, revealed: true };
    // The first reveal moves every player's run of right answers.
    const streaks = {};
    if (first) {
      const answers = s.answers.get(id);
      for (const [voter, player] of s.players) {
        const right = (answers?.get(voter)?.points ?? 0) > 0;
        streaks[`player:${voter}`] = { ...player, streak: right ? (player.streak ?? 0) + 1 : 0 };
      }
    }
    await this.ctx.storage.put({
      [`poll:${id}`]: {
        question: poll.question,
        options: poll.options,
        multiple: poll.multiple,
        image: poll.image,
        quiz,
      },
      revealed: id,
      ...streaks,
    });
    for (const [key, player] of Object.entries(streaks)) s.players.set(key.slice("player:".length), player);
    poll.quiz = quiz;
    s.revealed = id;
    // Every phone learns the answer; players also learn their result.
    const ranking = rank(s);
    this.send((voter) => {
      const player = voter && s.players.get(voter);
      const described = player ? describe(s, voter, player, ranking) : null;
      return {
        type: "result",
        poll: id,
        correct: quiz.correct,
        option: described?.last.option ?? null,
        points: described?.last.points ?? 0,
        player: described,
      };
    });
    await this.touch(s);
    return ok({ ok: true });
  }

  /** Remove a player, ban its phone from the session and free its name. */
  async kick(key, input) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    const s = await this.state();
    const name = typeof input?.name === "string" ? input.name.trim().toLocaleLowerCase() : "";
    const voter = name && s.names.get(name);
    if (!voter) {
      return fail(404, "no such player");
    }
    await this.ctx.storage.delete(`player:${voter}`);
    await this.ctx.storage.put(`banned:${voter}`, true);
    s.players.delete(voter);
    s.names.delete(name);
    s.banned.add(voter);
    for (const socket of this.ctx.getWebSockets("phone")) {
      if (socket.deserializeAttachment()?.voter !== voter) continue;
      try {
        socket.send(JSON.stringify({ type: "kicked" }));
        socket.close(4003, "removed by the presenter");
      } catch {
        // Already gone.
      }
    }
    await this.touch(s);
    return ok({ ok: true });
  }

  /** Start over: no polls, votes, players or bans; the key and the lobby
   * stay. */
  async reset(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    await this.restart(await this.state());
    return ok({ ok: true });
  }

  /** The session `s`, or a new game when the presentation that played the
   * last one ended: a presentation starts with nobody's points. */
  async startOver(s) {
    return s.stage === "end" ? this.restart(s) : s;
  }

  /** Forget the game in `s` and tell the phones; returns the new state. */
  async restart(s) {
    // Pictures stay: the presentation uploaded them once.
    const images = Object.fromEntries(await this.ctx.storage.list({ prefix: "image:" }));
    // Also removes the alarm; `touch` sets it again.
    await this.ctx.storage.deleteAll();
    const game = crypto.randomUUID();
    await this.ctx.storage.put({
      key: s.key,
      game,
      ...(s.lobby && { lobby: true }),
      ...(s.teams && { teams: s.teams }),
    });
    await putAll(this.ctx.storage, images);
    this.loading = Promise.resolve({
      ...s,
      stage: "play",
      current: null,
      latest: null,
      game,
      revealed: null,
      polls: new Map(),
      votes: new Map(),
      answers: new Map(),
      players: new Map(),
      names: new Map(),
      banned: new Set(),
      alarmAt: null,
    });
    const fresh = await this.state();
    await this.touch(fresh);
    await this.broadcast();
    this.send(() => ({ type: "player", player: null }));
    return fresh;
  }

  /** Store a poll's picture, named by `hash`. */
  async store(key, hash, mime, bytes) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    await this.ctx.storage.put(`image:${hash}`, { mime, bytes });
    await this.touch(await this.state());
    return ok({ ok: true });
  }

  /** A poll's picture, or null. */
  async image(hash) {
    return (await this.ctx.storage.get(`image:${hash}`)) ?? null;
  }

  /** Where the game is, as the presentation says: "play" while there are
   * questions ahead, "podium" once they are over, "end" when it ends. */
  async stage(key, input) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    const stage = input?.stage;
    if (!STAGES.has(stage)) {
      return fail(400, "invalid stage");
    }
    let s = await this.state();
    if (stage !== "end") s = await this.startOver(s);
    if (stage !== s.stage) {
      if (stage === "play") await this.ctx.storage.delete("stage");
      else await this.ctx.storage.put("stage", stage);
      s.stage = stage;
      await this.broadcast();
    }
    await this.touch(s);
    return ok({ ok: true });
  }

  /** The teams the game plays in, or none with no names. Declaring other
   * teams deals every player again. */
  async teams(key, input) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    const names = Array.isArray(input?.names) ? input.names.map((name) => text(name, MAX_TEAM_NAME)) : [];
    const colors = Array.isArray(input?.colors) ? input.colors : [];
    let teams = null;
    if (names.length > 0) {
      if (
        names.length < 2 ||
        names.length > MAX_TEAMS ||
        names.some((name) => !name) ||
        colors.length !== names.length ||
        colors.some((color) => typeof color !== "string" || !COLOR.test(color))
      ) {
        return fail(400, "invalid teams");
      }
      teams = { names, colors, choose: input.choose === true };
    }
    const s = await this.startOver(await this.state());
    if (JSON.stringify(teams) !== JSON.stringify(s.teams)) {
      if (teams) await this.ctx.storage.put("teams", teams);
      else await this.ctx.storage.delete("teams");
      s.teams = teams;
      // Players of other teams join again: dealt anew, or choosing.
      const changed = {};
      for (const [voter, player] of s.players) s.players.set(voter, { ...player, team: null });
      const joining = [...s.players].sort(([, a], [, b]) => (a.joined ?? 0) - (b.joined ?? 0));
      for (const [voter, player] of joining) {
        const next = { ...player, team: teams && !teams.choose ? smallestTeam(s) : null };
        s.players.set(voter, next);
        changed[`player:${voter}`] = next;
      }
      await putAll(this.ctx.storage, changed);
      await this.broadcast();
      this.send((voter) => {
        const player = voter && s.players.get(voter);
        return { type: "player", player: player ? describe(s, voter, player) : null };
      });
    }
    await this.touch(s);
    return ok({ ok: true });
  }

  /** Whether phones join as soon as they open the page, for a
   * presentation that shows its audience. */
  async lobby(key, input) {
    const denied = await this.authorize(key, true);
    if (denied) return denied;
    const s = await this.startOver(await this.state());
    const open = input?.open === true;
    if (open !== s.lobby) {
      if (open) await this.ctx.storage.put("lobby", true);
      else await this.ctx.storage.delete("lobby");
      s.lobby = open;
      await this.broadcast();
    }
    await this.touch(s);
    return ok({ ok: true });
  }

  async results(key) {
    const denied = await this.authorize(key, false);
    if (denied) return denied;
    return ok(await this.summary(true));
  }

  /** What the presentation reads: counts, quizzes, players, audience. */
  /** What the presentation reads. `all` sends every poll's answers, as a
   * presentation that just connected needs; pushes send the open poll's. */
  async summary(all = false) {
    const s = await this.state();
    const polls = {};
    const names = new Map([...s.players].map(([voter, player]) => [voter, player.name]));
    for (const [id, poll] of s.polls) {
      const chosen = poll.quiz ? s.answers.get(id) : s.votes.get(id);
      polls[id] = {
        open: id === s.current,
        counts: poll.counts,
        total: poll.counts.reduce((sum, count) => sum + count, 0),
        respondents: chosen?.size ?? 0,
      };
      if (all || id === s.current) {
        // Each player's answer, for the scene to show who chose what.
        polls[id].answers = [...(chosen ?? [])]
          .filter(([voter]) => names.has(voter))
          .map(([voter, answer]) => {
            const option = typeof answer === "object" && !Array.isArray(answer) ? answer.option : answer;
            return {
              name: names.get(voter),
              options: choices(option),
              elapsed: answer?.elapsed ?? 0,
              points: answer?.points ?? 0,
              right: poll.quiz ? (answer?.points ?? 0) > 0 : null,
            };
          });
      }
      if (poll.quiz) {
        polls[id].quiz = {
          correct: poll.quiz.correct,
          deadline: poll.quiz.deadline,
          revealed: poll.quiz.revealed,
        };
      }
    }
    const ranking = rank(s);
    const players = ranking.slice(0, LEADERBOARD).map((player) => ({
      name: player.name,
      score: player.score,
      correct: player.correct,
      answered: player.answered,
      streak: player.streak ?? 0,
      avatar: player.avatar ?? defaultAvatar(player.voter),
      team: player.team ?? null,
    }));
    // Everyone in the order they joined, for the scene's audience.
    const audience = ranking
      .map((player) => ({
        name: player.name,
        joined: player.joined ?? 0,
        avatar: player.avatar ?? defaultAvatar(player.voter),
        team: player.team ?? null,
        score: player.score,
        correct: player.correct,
        answered: player.answered,
        streak: player.streak ?? 0,
      }))
      .sort((a, b) => a.joined - b.joined || a.name.localeCompare(b.name))
      .slice(0, AUDIENCE);
    return {
      current: s.current,
      latest: s.latest,
      game: s.game,
      connected: this.ctx.getWebSockets("phone").length,
      now: Date.now(),
      polls,
      players,
      playerCount: ranking.length,
      audience,
      // Each team's standing, in the scene's order.
      teams: s.teams ? teamTotals(s) : [],
    };
  }

  // The first presenter request claims the session with its key.
  async authorize(key, claim) {
    if (typeof key !== "string" || !KEY.test(key)) {
      return fail(401, "missing presenter key");
    }
    const s = await this.state();
    if (s.key && key === this.verified) return null;
    const hash = await sha256(key);
    if (!s.key) {
      if (!claim) return fail(401, "unknown session");
      // A claimed session starts its first game.
      const game = s.game ?? crypto.randomUUID();
      await this.ctx.storage.put({ key: hash, game });
      s.key = hash;
      s.game = game;
    }
    if (s.key !== hash) return fail(403, "wrong presenter key");
    this.verified = key;
    return null;
  }

  /** Record activity: tell the presentation, and keep the session for
   * SESSION_TTL_MS. Each `setAlarm` is a billed write, so it moves only
   * once the alarm is ALARM_SLACK_MS behind. */
  async touch(s) {
    this.schedulePush();
    const due = Date.now() + SESSION_TTL_MS;
    if (s.alarmAt !== null && due - s.alarmAt < ALARM_SLACK_MS) return;
    await this.ctx.storage.setAlarm(due);
    s.alarmAt = due;
  }

  async alarm() {
    await this.ctx.storage.deleteAll();
    this.loading = null;
    this.verified = null;
  }
}

/** The vote counts of a poll, from its votes or a quiz's answers: each
 * answer chosen counts, so a multiple choice poll adds up to more votes
 * than voters. */
function countVotes(s, id, poll) {
  const counts = poll.options.map(() => 0);
  const chosen = poll.quiz ? s.answers.get(id) : s.votes.get(id);
  for (const choice of chosen?.values() ?? []) {
    const option = typeof choice === "object" && !Array.isArray(choice) ? choice.option : choice;
    for (const index of choices(option)) {
      if (index >= 0 && index < counts.length) counts[index] += 1;
    }
  }
  return counts;
}

/** The answers of a choice: an index, or a list of them. */
function choices(choice) {
  if (Array.isArray(choice)) return choice;
  return Number.isInteger(choice) ? [choice] : [];
}

/** Whether two choices pick the same answers. */
function sameChoice(a, b) {
  const x = [...choices(a)].sort((m, n) => m - n);
  const y = [...choices(b)].sort((m, n) => m - n);
  return x.length === y.length && x.every((value, index) => value === y[index]);
}

/** `value` as a choice of `poll`: one index, or for a multiple choice
 * poll a list of distinct indexes, sorted; null if it is neither. */
function checkChoice(value, poll) {
  const count = poll.options.length;
  const valid = (index) => Number.isInteger(index) && index >= 0 && index < count;
  if (!poll.multiple) {
    if (Array.isArray(value) && value.length === 1) value = value[0];
    return valid(value) ? value : null;
  }
  const list = Array.isArray(value) ? value : [value];
  const distinct = [...new Set(list)];
  if (distinct.length === 0 || distinct.length !== list.length || !distinct.every(valid)) return null;
  return distinct.sort((a, b) => a - b);
}

/** What `voter` chose on poll `id`: its vote, or its quiz answer. */
function choiceOf(s, id, voter) {
  if (!id || !voter) return null;
  const choice = s.polls.get(id)?.quiz ? s.answers.get(id)?.get(voter)?.option : s.votes.get(id)?.get(voter);
  return choice ?? null;
}

/** Every player, best first; ties go to the most correct answers. */
function rank(s) {
  const players = [];
  for (const [voter, player] of s.players) players.push({ voter, ...player });
  players.sort(
    (a, b) => b.score - a.score || b.correct - a.correct || a.name.localeCompare(b.name),
  );
  return players;
}

/** A player as the phone sees it: with its place among all players and its
 * result on the quiz open now, if revealed, or else on the last one
 * revealed. */
function describe(s, voter, player, ranking = rank(s)) {
  const place = ranking.findIndex((entry) => entry.voter === voter) + 1;
  const open = s.current && s.polls.get(s.current);
  const id = open?.quiz?.revealed ? s.current : s.revealed;
  const quiz = id && s.polls.get(id)?.quiz;
  const answer = quiz?.revealed ? s.answers.get(id)?.get(voter) : null;
  // A quiz still taking answers keeps its points secret: a phone showing a
  // new score would tell its neighbors the answer.
  const hidden = open?.quiz && !open.quiz.revealed ? s.answers.get(s.current)?.get(voter) : null;
  return {
    name: player.name,
    avatar: player.avatar ?? defaultAvatar(voter),
    score: player.score - (hidden?.points ?? 0),
    correct: player.correct - (hidden?.points > 0 ? 1 : 0),
    answered: player.answered,
    rank: place || null,
    players: ranking.length,
    team: player.team ?? null,
    last: quiz?.revealed
      ? { poll: id, correct: quiz.correct, option: answer?.option ?? null, points: answer?.points ?? 0 }
      : null,
  };
}

/** Each team's points and players, in the scene's order. */
function teamTotals(s) {
  const totals = s.teams.names.map(() => ({ score: 0, players: 0 }));
  for (const player of s.players.values()) {
    const total = totals[player.team];
    if (!total) continue;
    total.score += player.score;
    total.players += 1;
  }
  return totals;
}

/** The teams best first, for the phones' final screen. */
function teamStandings(s) {
  return teamTotals(s)
    .map((total, team) => ({ team, name: s.teams.names[team], color: s.teams.colors[team], ...total }))
    .sort((a, b) => b.score - a.score || a.team - b.team);
}

/** The team with the fewest players, the first on a tie. */
function smallestTeam(s) {
  const sizes = s.teams.names.map(() => 0);
  for (const player of s.players.values()) {
    if (Number.isInteger(player.team) && player.team < sizes.length) sizes[player.team] += 1;
  }
  return sizes.indexOf(Math.min(...sizes));
}

/** The lists a character indexes, in its order. */
const AVATAR_PARTS = ["bodies", "colors", "eyes", "mouths", "extras"];

/** `value` if it is a character of the catalog, else null. */
function checkAvatar(value) {
  const valid =
    Array.isArray(value) &&
    value.length === AVATAR_PARTS.length &&
    value.every(
      (index, part) =>
        Number.isInteger(index) && index >= 0 && index < avatarParts[AVATAR_PARTS[part]].length,
    );
  return valid ? [...value] : null;
}

/** The character of a phone that did not choose one, the same every time:
 * read from its voter id. */
function defaultAvatar(voter) {
  return AVATAR_PARTS.map(
    (part, index) =>
      parseInt(voter.slice(index * 4, index * 4 + 4), 16) % avatarParts[part].length,
  );
}

/** Storage takes at most 128 keys per call. */
const STORAGE_BATCH = 128;

async function putAll(storage, entries) {
  const pairs = Object.entries(entries);
  for (let start = 0; start < pairs.length; start += STORAGE_BATCH) {
    await storage.put(Object.fromEntries(pairs.slice(start, start + STORAGE_BATCH)));
  }
}

async function deleteAll(storage, keys) {
  for (let start = 0; start < keys.length; start += STORAGE_BATCH) {
    await storage.delete(keys.slice(start, start + STORAGE_BATCH));
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

/** The request's JSON body, read up to MAX_BODY bytes. */
async function body(request) {
  if (Number(request.headers.get("content-length") ?? 0) > MAX_BODY) {
    throw new Error("request too large");
  }
  const reader = request.body?.getReader();
  if (!reader) return {};
  const chunks = [];
  let size = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    size += value.byteLength;
    if (size > MAX_BODY) {
      await reader.cancel();
      throw new Error("request too large");
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  const raw = new TextDecoder().decode(bytes);
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
