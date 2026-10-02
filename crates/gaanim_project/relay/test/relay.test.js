// End-to-end tests: start the relay with `wrangler dev` on a free port and
// fresh storage, then act as the presentation and as phones.

import { after, before, describe, test } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const wranglerBin = join(root, "node_modules", "wrangler", "bin", "wrangler.js");

let base;
let server;
let storage;

function freePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address();
      probe.close(() => resolve(port));
    });
  });
}

before(async () => {
  const port = await freePort();
  storage = mkdtempSync(join(tmpdir(), "gaanim-relay-"));
  base = `http://127.0.0.1:${port}`;
  server = spawn(
    process.execPath,
    [wranglerBin, "dev", "--ip", "127.0.0.1", "--port", String(port), "--persist-to", storage],
    { cwd: root, env: { ...process.env, WRANGLER_SEND_METRICS: "false" } },
  );
  let log = "";
  await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`wrangler dev did not start:\n${log}`)), 90_000);
    const watch = (chunk) => {
      log += chunk;
      if (log.includes("Ready on")) {
        clearTimeout(timeout);
        resolve();
      }
    };
    server.stdout.on("data", watch);
    server.stderr.on("data", watch);
    server.once("exit", (code) => reject(new Error(`wrangler dev exited (${code}):\n${log}`)));
  });
});

after(() => {
  server?.kill();
  try {
    rmSync(storage, { recursive: true, force: true });
  } catch {
    // workerd may still hold the files for a moment on Windows.
  }
});

let codeCounter = 0;
/** A fresh session code per test, from the relay's alphabet. */
function newCode() {
  const alphabet = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
  codeCounter += 1;
  return `TST${alphabet[Math.floor(codeCounter / 32) % 32]}${alphabet[codeCounter % 32]}A`;
}

const key = (seed) => seed.repeat(64).slice(0, 64);
const voter = (seed) => seed.repeat(32).slice(0, 32);

function presenter(code, secret = key("ab")) {
  const auth = { authorization: `Bearer ${secret}` };
  return {
    open: (id, question, options) =>
      fetch(`${base}/s/${code}/poll`, {
        method: "PUT",
        headers: { ...auth, "content-type": "application/json" },
        body: JSON.stringify({ id, question, options }),
      }),
    close: () => fetch(`${base}/s/${code}/poll`, { method: "DELETE", headers: auth }),
    results: () => fetch(`${base}/s/${code}/results`, { headers: auth }),
    quiz: (id, question, options, quiz) =>
      fetch(`${base}/s/${code}/poll`, {
        method: "PUT",
        headers: { ...auth, "content-type": "application/json" },
        body: JSON.stringify({ id, question, options, ...quiz }),
      }),
    post: (path, value) =>
      fetch(`${base}/s/${code}/${path}`, {
        method: "POST",
        headers: { ...auth, "content-type": "application/json" },
        body: JSON.stringify(value ?? {}),
      }),
  };
}

function joinGame(code, id, name, extra = {}) {
  return fetch(`${base}/s/${code}/join`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ voter: id, name, ...extra }),
  });
}

/** The poll part of the presenter's results, without each player's answers. */
function pollResults({ current, connected, polls }) {
  const counts = Object.fromEntries(
    Object.entries(polls).map(([id, { open, counts, total }]) => [id, { open, counts, total }]),
  );
  return { current, connected, polls: counts };
}

/** Assert that `actual` has every field of `expected`, with its value. */
function like(actual, expected, message) {
  const picked = Object.fromEntries(Object.keys(expected).map((name) => [name, actual?.[name]]));
  assert.deepEqual(picked, expected, message);
}

function phone(code, id) {
  return {
    current: async () => (await fetch(`${base}/s/${code}/poll?voter=${id}`)).json(),
    vote: (poll, option) =>
      fetch(`${base}/s/${code}/vote`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ poll, option, voter: id }),
      }),
  };
}

describe("pages", () => {
  test("the join page is served at the root", async () => {
    const response = await fetch(`${base}/`);
    assert.equal(response.status, 200);
    assert.match(response.headers.get("content-type"), /text\/html/);
    const html = await response.text();
    assert.match(html, /id="join"/);
    assert.match(response.headers.get("content-security-policy") ?? "", /default-src 'self'/);
  });

  test("a session code opens the voting page", async () => {
    const response = await fetch(`${base}/s/ABC234`);
    assert.equal(response.status, 200);
    assert.match(await response.text(), /id="answers"/);
    assert.equal(response.headers.get("cache-control"), "no-store");
    assert.equal(response.headers.get("x-frame-options"), "DENY");
  });

  test("a lowercase code redirects to the canonical address", async () => {
    const response = await fetch(`${base}/s/abc234`, { redirect: "manual" });
    assert.equal(response.status, 302);
    assert.equal(response.headers.get("location"), `${base}/s/ABC234`);
  });

  test("codes with confusable characters do not exist", async () => {
    for (const code of ["ABC10O", "ABCDI2", "ABC23", "ABC2345"]) {
      assert.equal((await fetch(`${base}/s/${code}`)).status, 404, code);
    }
  });

  test("health reports the API version", async () => {
    assert.deepEqual(await (await fetch(`${base}/health`)).json(), { relay: "gaanim", version: 11 });
  });
});

describe("votes", () => {
  test("phones vote once each and may change their vote", async () => {
    const code = newCode();
    const host = presenter(code);
    const opened = await host.open("p0", " ¿Cuál? ", ["A", "B", "C"]);
    assert.equal(opened.status, 200);
    assert.deepEqual(await opened.json(), { id: "p0" });

    const poll = await phone(code, voter("a")).current();
    like(poll, { open: true, id: "p0", question: "¿Cuál?", options: ["A", "B", "C"], chosen: null });

    assert.equal((await phone(code, voter("a")).vote("p0", 0)).status, 200);
    assert.equal((await phone(code, voter("b")).vote("p0", 1)).status, 200);
    assert.equal((await phone(code, voter("c")).vote("p0", 1)).status, 200);
    assert.equal((await phone(code, voter("c")).vote("p0", 2)).status, 200);
    assert.equal((await phone(code, voter("c")).vote("p0", 2)).status, 200);

    const results = pollResults(await (await host.results()).json());
    assert.deepEqual(results, {
      current: "p0",
      connected: 0,
      polls: { p0: { open: true, counts: [1, 1, 1], total: 3 } },
    });
  });

  test("invalid votes are refused", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p0", "Q", ["A", "B"]);
    const someone = phone(code, voter("d"));
    assert.equal((await someone.vote("p0", 2)).status, 400);
    assert.equal((await someone.vote("p0", -1)).status, 400);
    assert.equal((await someone.vote("p0", 0.5)).status, 400);
    assert.equal((await phone(code, "short").vote("p0", 0)).status, 400);
    assert.equal((await someone.vote("p1", 0)).status, 409);
  });

  test("each poll keeps its votes when the presentation comes back", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p0", "One", ["A", "B"]);
    await phone(code, voter("e")).vote("p0", 0);
    await host.open("p1", "Two", ["X", "Y", "Z"]);
    assert.equal((await phone(code, voter("e")).vote("p0", 1)).status, 409);
    await phone(code, voter("e")).vote("p1", 2);

    await host.open("p0", "One", ["A", "B"]);
    assert.equal((await phone(code, voter("f")).vote("p0", 1)).status, 200);
    const results = pollResults(await (await host.results()).json());
    assert.deepEqual(results, {
      current: "p0",
      connected: 0,
      polls: {
        p0: { open: true, counts: [1, 1], total: 2 },
        p1: { open: false, counts: [0, 0, 1], total: 1 },
      },
    });
  });

  test("a poll whose text changed starts from zero", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p0", "One", ["A", "B"]);
    await phone(code, voter("7")).vote("p0", 1);
    await host.open("p0", "One, edited", ["A", "B"]);
    const results = await (await host.results()).json();
    like(results.polls.p0, { open: true, counts: [0, 0], total: 0 });
    // The phone's old vote no longer counts, so it can vote again.
    assert.equal((await phone(code, voter("7")).vote("p0", 1)).status, 200);
    assert.deepEqual((await (await host.results()).json()).polls.p0.counts, [0, 1]);
  });

  test("closing a question hides it from phones", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p0", "Q", ["A", "B"]);
    assert.equal((await host.close()).status, 200);
    like(await phone(code, voter("8")).current(), { open: false });
    assert.equal((await phone(code, voter("8")).vote("p0", 0)).status, 409);
    const results = await (await host.results()).json();
    assert.equal(results.current, null);
    assert.equal(results.polls.p0.open, false);
  });
});

/** A phone's WebSocket, with a queue of the messages it received. */
async function socket(code) {
  const ws = new WebSocket(`${base.replace("http", "ws")}/s/${code}/ws`);
  const received = [];
  const waiting = [];
  ws.addEventListener("message", (event) => {
    const message = event.data === "pong" ? "pong" : JSON.parse(event.data);
    const index = waiting.findIndex((waiter) => waiter.match(message));
    if (index >= 0) waiting.splice(index, 1)[0].resolve(message);
    else received.push(message);
  });
  await new Promise((resolve, reject) => {
    ws.addEventListener("open", resolve);
    ws.addEventListener("error", () => reject(new Error("the socket did not open")));
  });
  return {
    send: (value) => ws.send(typeof value === "string" ? value : JSON.stringify(value)),
    close: () => ws.close(),
    /** The next message `match` accepts, received or still to come. */
    next(match = () => true) {
      const index = received.findIndex(match);
      if (index >= 0) return Promise.resolve(received.splice(index, 1)[0]);
      return new Promise((resolve, reject) => {
        const waiter = { match, resolve };
        waiting.push(waiter);
        setTimeout(() => {
          waiting.splice(waiting.indexOf(waiter) >>> 0, 1);
          reject(new Error("no such message"));
        }, 5000);
      });
    },
  };
}

const pollMessage = (message) => message?.type === "poll";

describe("websockets", () => {
  test("phones get the question on connect and on every change", async () => {
    const code = newCode();
    const host = presenter(code);
    const first = await socket(code);
    const second = await socket(code);
    like(await first.next(pollMessage), { type: "poll", open: false });
    like(await second.next(pollMessage), { type: "poll", open: false });

    await host.open("p0", "¿Cuál?", ["A", "B"]);
    const opened = { type: "poll", open: true, id: "p0", question: "¿Cuál?", options: ["A", "B"] };
    like(await first.next(pollMessage), opened);
    like(await second.next(pollMessage), opened);

    // A phone that connects late gets the open question right away.
    const late = await socket(code);
    like(await late.next(pollMessage), opened);

    await host.close();
    like(await first.next(pollMessage), { type: "poll", open: false });
    for (const phone of [first, second, late]) phone.close();
  });

  test("votes travel on the socket and count like any other", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p0", "Q", ["A", "B"]);
    const phone = await socket(code);
    const reply = (message) => message?.type === "voted" || message?.type === "error";

    phone.send({ type: "vote", poll: "p0", option: 1, voter: voter("a") });
    assert.deepEqual(await phone.next(reply), { type: "voted", poll: "p0", option: 1 });
    phone.send({ type: "vote", poll: "p0", option: 5, voter: voter("a") });
    assert.deepEqual(await phone.next(reply), {
      type: "error",
      status: 400,
      error: "unknown answer",
      poll: "p0",
    });
    phone.send({ type: "vote", poll: "p9", option: 0, voter: voter("a") });
    assert.equal((await phone.next(reply)).status, 409);
    phone.send("not json");
    assert.equal((await phone.next(reply)).status, 400);

    const results = await (await host.results()).json();
    assert.deepEqual(results.polls.p0.counts, [0, 1]);
    assert.equal(results.connected, 1);
    phone.close();
  });

  test("keepalive pings are answered", async () => {
    const phone = await socket(newCode());
    phone.send("ping");
    assert.equal(await phone.next((message) => message === "pong"), "pong");
    phone.close();
  });

  test("a plain request for the socket is refused", async () => {
    assert.equal((await fetch(`${base}/s/ABC234/ws`)).status, 426);
  });

  test("the voting page may open its own socket", async () => {
    const response = await fetch(`${base}/s/ABC234`);
    const policy = response.headers.get("content-security-policy") ?? "";
    assert.match(policy, new RegExp(`connect-src 'self' ${base.replace("http", "ws")}`));
    assert.match(policy, /frame-ancestors 'none'/);
  });
});

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const reply = (message) => ["voted", "error", "joined", "player"].includes(message?.type);

describe("quiz", () => {
  test("correct answers score by speed and each player answers once", async () => {
    const code = newCode();
    const host = presenter(code);
    assert.equal((await host.quiz("q1", "2 + 2", ["3", "4"], { correct: 1, time: 10 })).status, 200);
    assert.equal((await joinGame(code, voter("a"), " Ana  María ")).status, 200);
    assert.equal((await joinGame(code, voter("b"), "Beto")).status, 200);

    assert.equal((await phone(code, voter("a")).vote("q1", 1)).status, 200);
    const again = await phone(code, voter("a")).vote("q1", 0);
    assert.equal(again.status, 409);
    assert.equal((await again.json()).error, "already answered");
    assert.equal((await phone(code, voter("b")).vote("q1", 0)).status, 200);
    const stranger = await phone(code, voter("c")).vote("q1", 1);
    assert.equal(stranger.status, 401);

    const results = await (await host.results()).json();
    assert.deepEqual(results.polls.q1.counts, [1, 1]);
    assert.equal(results.polls.q1.quiz.correct, 1);
    assert.equal(results.playerCount, 2);
    const [first, second] = results.players;
    assert.equal(first.name, "Ana María");
    assert.ok(first.score > 900 && first.score <= 1000, `fast answer scores ${first.score}`);
    like(second, { name: "Beto", score: 0, correct: 0, answered: 1 });
  });

  test("answers after the time are refused", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q1", "Fast?", ["Yes", "No"], { correct: 0, time: 5 });
    await joinGame(code, voter("a"), "Ana");
    await sleep(5600);
    const late = await phone(code, voter("a")).vote("q1", 0);
    assert.equal(late.status, 409);
    assert.equal((await late.json()).error, "time is up");
  });

  test("names are checked and unique", async () => {
    const code = newCode();
    for (const name of ["A", "x".repeat(21), "<b>hi</b>", "  ", "-dash"]) {
      assert.equal((await joinGame(code, voter("a"), name)).status, 400, name);
    }
    assert.equal((await joinGame(code, voter("a"), "Zoë 2")).status, 200);
    assert.equal((await joinGame(code, voter("b"), "zoë 2")).status, 409);
    // Renaming frees the old name.
    assert.equal((await joinGame(code, voter("a"), "Zed")).status, 200);
    assert.equal((await joinGame(code, voter("b"), "Zoë 2")).status, 200);
  });

  test("a reveal tells each phone how it did", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q1", "Capital of Peru?", ["Lima", "Cusco"], { correct: 0, time: 30, points: 500 });
    const ana = await socket(code);
    const beto = await socket(code);
    const watcher = await socket(code);
    ana.send({ type: "join", voter: voter("a"), name: "Ana" });
    assert.equal((await ana.next(reply)).type, "joined");
    beto.send({ type: "join", voter: voter("b"), name: "Beto" });
    await beto.next(reply);
    ana.send({ type: "vote", poll: "q1", option: 0, voter: voter("a") });
    assert.equal((await ana.next(reply)).type, "voted");
    beto.send({ type: "vote", poll: "q1", option: 1, voter: voter("b") });
    await beto.next(reply);

    assert.equal((await host.post("reveal", { id: "q1" })).status, 200);
    const result = (message) => message?.type === "result";
    const forAna = await ana.next(result);
    assert.equal(forAna.correct, 0);
    assert.equal(forAna.option, 0);
    assert.ok(forAna.points > 250 && forAna.points <= 500);
    assert.equal(forAna.player.rank, 1);
    assert.equal(forAna.player.players, 2);
    const forBeto = await beto.next(result);
    assert.deepEqual([forBeto.option, forBeto.points, forBeto.player.rank], [1, 0, 2]);
    assert.deepEqual(await watcher.next(result), {
      type: "result",
      poll: "q1",
      correct: 0,
      option: null,
      points: 0,
      player: null,
    });

    // Answers are closed, and a phone that comes back finds its result.
    assert.equal((await phone(code, voter("b")).vote("q1", 0)).status, 409);
    const player = await (await fetch(`${base}/s/${code}/player?voter=${voter("b")}`)).json();
    assert.deepEqual(player.player.last, { poll: "q1", correct: 0, option: 1, points: 0 });
    for (const phone of [ana, beto, watcher]) phone.close();
  });

  test("changing a quiz takes back the points it gave", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q1", "Q", ["A", "B"], { correct: 0, time: 30 });
    await joinGame(code, voter("a"), "Ana");
    await phone(code, voter("a")).vote("q1", 0);
    assert.ok((await (await host.results()).json()).players[0].score > 0);
    await host.quiz("q1", "Q", ["A", "B"], { correct: 1, time: 30 });
    const results = await (await host.results()).json();
    like(results.players[0], { name: "Ana", score: 0, correct: 0, answered: 0 });
    assert.deepEqual(results.polls.q1.counts, [0, 0]);
  });

  test("a kicked player is removed and cannot come back", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q1", "Q", ["A", "B"], { correct: 0, time: 30 });
    const troll = await socket(code);
    troll.send({ type: "join", voter: voter("d"), name: "Troll" });
    await troll.next(reply);
    assert.equal((await host.post("kick", { name: "troll" })).status, 200);
    assert.deepEqual(await troll.next((message) => message?.type === "kicked"), { type: "kicked" });
    assert.equal((await joinGame(code, voter("d"), "Nice")).status, 403);
    assert.equal((await host.post("kick", { name: "nobody" })).status, 404);
    assert.equal((await (await host.results()).json()).playerCount, 0);
  });

  test("a reset starts the session over", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q1", "Q", ["A", "B"], { correct: 0, time: 30 });
    await joinGame(code, voter("a"), "Ana");
    const watcher = await socket(code);
    await watcher.next(pollMessage);
    assert.equal((await host.post("reset")).status, 200);
    like(await watcher.next(pollMessage), { type: "poll", open: false });
    const results = await (await host.results()).json();
    assert.deepEqual([results.polls, results.players, results.current], [{}, [], null]);
    // The key still owns the session.
    assert.equal((await presenter(code, key("cd")).results()).status, 403);
    watcher.close();
  });

  test("malformed quizzes are refused", async () => {
    const host = presenter(newCode());
    for (const quiz of [
      { correct: 2 },
      { correct: -1 },
      { correct: 0, time: 2 },
      { correct: 0, time: 1000 },
      { correct: 0, points: 5 },
      { correct: 0.5 },
    ]) {
      assert.equal((await host.quiz("q1", "Q", ["A", "B"], quiz)).status, 400, JSON.stringify(quiz));
    }
  });
});

describe("presenter", () => {
  test("the first key claims a session and no other key can use it", async () => {
    const code = newCode();
    assert.equal((await presenter(code).results()).status, 401);
    assert.equal((await presenter(code).open("p0", "Q", ["A", "B"])).status, 200);
    const intruder = presenter(code, key("cd"));
    assert.equal((await intruder.results()).status, 403);
    assert.equal((await intruder.open("p0", "Mine", ["A", "B"])).status, 403);
    assert.equal((await intruder.close()).status, 403);
    assert.equal((await presenter(code, "short").results()).status, 401);
  });

  test("malformed questions are refused", async () => {
    const code = newCode();
    const host = presenter(code);
    assert.equal((await host.open("p0", "", ["A", "B"])).status, 400);
    assert.equal((await host.open("p0", "Q", ["A"])).status, 400);
    assert.equal((await host.open("p0", "Q", ["1", "2", "3", "4", "5", "6", "7"])).status, 400);
    assert.equal((await host.open("p0", "Q", ["A", " "])).status, 400);
    assert.equal((await host.open("", "Q", ["A", "B"])).status, 400);
    assert.equal((await host.open("Bad Id!", "Q", ["A", "B"])).status, 400);
    const huge = await fetch(`${base}/s/${code}/poll`, {
      method: "PUT",
      headers: { authorization: `Bearer ${key("ab")}`, "content-type": "application/json" },
      body: JSON.stringify({ id: "p0", question: "x".repeat(9000), options: ["A", "B"] }),
    });
    assert.equal(huge.status, 400);
  });
});

describe("multiple choice", () => {
  test("a poll may take several answers from each phone", async () => {
    const code = newCode();
    const host = presenter(code);
    const opened = await fetch(`${base}/s/${code}/poll`, {
      method: "PUT",
      headers: { authorization: `Bearer ${key("ab")}`, "content-type": "application/json" },
      body: JSON.stringify({ id: "m", question: "Q", options: ["A", "B", "C"], multiple: true }),
    });
    assert.equal(opened.status, 200);
    assert.equal((await phone(code, voter("a")).vote("m", [0, 2])).status, 200);
    assert.equal((await phone(code, voter("b")).vote("m", [2])).status, 200);
    assert.equal((await phone(code, voter("a")).vote("m", [1])).status, 200);
    assert.equal((await phone(code, voter("c")).vote("m", [0, 0])).status, 400);
    like((await (await host.results()).json()).polls.m, { counts: [0, 1, 1], respondents: 2 });
  });

  test("a quiz with several right answers scores only all of them", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q", "Q", ["A", "B", "C"], { correct: [0, 2], time: 30 });
    for (const [seed, name] of [["a", "Ana"], ["b", "Beto"]]) await joinGame(code, voter(seed), name);
    await phone(code, voter("a")).vote("q", [0, 2]);
    await phone(code, voter("b")).vote("q", [0]);
    const results = await (await host.results()).json();
    const score = Object.fromEntries(results.players.map((player) => [player.name, player.score]));
    assert.ok(score.Ana > 0);
    assert.equal(score.Beto, 0);
    // Each player's answer, for the scene to show who chose what.
    const answers = Object.fromEntries(results.polls.q.answers.map((answer) => [answer.name, answer]));
    like(answers.Ana, { options: [0, 2], right: true });
    like(answers.Beto, { options: [0], right: false });
  });
});

describe("lobby, teams and stages", () => {
  test("a lobby asks phones for a nickname right away", async () => {
    const code = newCode();
    const host = presenter(code);
    const watcher = await socket(code);
    like(await watcher.next(pollMessage), { lobby: false });
    assert.equal((await host.post("lobby", { open: true })).status, 200);
    like(await watcher.next(pollMessage), { lobby: true, open: false });
    await joinGame(code, voter("a"), "Ana");
    await joinGame(code, voter("b"), "Beto");
    const { audience } = await (await host.results()).json();
    assert.deepEqual(
      audience.map((player) => player.name),
      ["Ana", "Beto"],
    );
    assert.equal(audience[0].avatar.length, 5);
    watcher.close();
  });

  test("chosen teams need a choice and add up their players' points", async () => {
    const code = newCode();
    const host = presenter(code);
    const teams = { names: ["Rojo", "Azul"], colors: ["#ff0000", "#0000ff"], choose: true };
    assert.equal((await host.post("teams", teams)).status, 200);
    const unchosen = await joinGame(code, voter("a"), "Ana");
    assert.equal(unchosen.status, 400);
    assert.equal((await unchosen.json()).error, "choose a team");
    assert.equal((await (await joinGame(code, voter("a"), "Ana", { team: 1 })).json()).player.team, 1);
    await joinGame(code, voter("b"), "Beto", { team: 1 });
    await host.quiz("q", "Q", ["A", "B"], { correct: 0, time: 30 });
    await phone(code, voter("a")).vote("q", 0);
    const results = await (await host.results()).json();
    assert.equal(results.teams[0].players, 0);
    assert.equal(results.teams[1].players, 2);
    assert.ok(results.teams[1].score > 0);
    assert.equal((await host.post("teams", { ...teams, colors: ["red", "blue"] })).status, 400);
  });

  test("dealt teams fill up evenly", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.post("teams", { names: ["A", "B"], colors: ["#111111", "#222222"] });
    const teams = [];
    for (const seed of ["a", "b", "c", "d"]) {
      const joined = await joinGame(code, voter(seed), `Player ${seed}`);
      teams.push((await joined.json()).player.team);
    }
    assert.deepEqual(teams, [0, 1, 0, 1]);
  });

  test("phones show the podium once the questions are over", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.quiz("q", "Q", ["A", "B"], { correct: 0, time: 30 });
    await joinGame(code, voter("a"), "Ana");
    await phone(code, voter("a")).vote("q", 0);
    await host.close();
    assert.equal((await host.post("stage", { stage: "podium" })).status, 200);
    const shown = await phone(code, voter("a")).current();
    like(shown, { stage: "podium", players: 1 });
    assert.equal(shown.podium[0].name, "Ana");
    assert.equal((await host.post("stage", { stage: "later" })).status, 400);
    // After the end, the next presentation starts a new game.
    await host.post("stage", { stage: "end" });
    await host.open("p", "Q", ["A", "B"]);
    assert.equal((await (await host.results()).json()).playerCount, 0);
  });
});

describe("pictures", () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);
  const put = (code, hash, type = "image/png", secret = key("ab")) =>
    fetch(`${base}/s/${code}/image/${hash}`, {
      method: "PUT",
      headers: { authorization: `Bearer ${secret}`, "content-type": type },
      body: png,
    });

  test("the presenter stores a question's picture and phones load it", async () => {
    const code = newCode();
    const hash = "0123456789abcdef";
    assert.equal((await put(code, hash)).status, 200);
    assert.equal((await put(code, hash, "text/html")).status, 400);
    assert.equal((await put(code, hash, "image/png", key("cd"))).status, 403);
    const opened = await fetch(`${base}/s/${code}/poll`, {
      method: "PUT",
      headers: { authorization: `Bearer ${key("ab")}`, "content-type": "application/json" },
      body: JSON.stringify({ id: "p", question: "Q", options: ["A", "B"], image: hash }),
    });
    assert.equal(opened.status, 200);
    like(await phone(code, voter("a")).current(), { image: hash });
    const loaded = await fetch(`${base}/s/${code}/image/${hash}`);
    assert.equal(loaded.headers.get("content-type"), "image/png");
    assert.deepEqual(new Uint8Array(await loaded.arrayBuffer()), png);
    // A new game keeps the pictures.
    await presenter(code).post("reset");
    assert.equal((await fetch(`${base}/s/${code}/image/${hash}`)).status, 200);
    assert.equal((await fetch(`${base}/s/${code}/image/fedcba9876543210`)).status, 404);
  });
});

describe("presenter socket", () => {
  test("results are pushed on connect and when votes arrive", async () => {
    const code = newCode();
    const host = presenter(code);
    await host.open("p", "Q", ["A", "B"]);
    const ws = new WebSocket(`${base.replace("http", "ws")}/s/${code}/presenter`, {
      headers: { authorization: `Bearer ${key("ab")}` },
    });
    const messages = [];
    ws.addEventListener("message", (event) => messages.push(JSON.parse(event.data)));
    const until = async (check) => {
      const deadline = Date.now() + 5000;
      while (!messages.some(check)) {
        if (Date.now() > deadline) throw new Error("no such push");
        await sleep(50);
      }
    };
    await until((message) => message.type === "results" && message.current === "p");
    await phone(code, voter("a")).vote("p", 1);
    await until((message) => message.polls?.p?.counts?.[1] === 1);
    ws.close();
  });
});

describe("limits", () => {
  test("a socket speaks for one phone", async () => {
    const code = newCode();
    await presenter(code).open("p", "Q", ["A", "B"]);
    const phone = await socket(code);
    phone.send({ type: "hello", voter: voter("a") });
    await phone.next((message) => message?.type === "player");
    phone.send({ type: "vote", poll: "p", option: 0, voter: voter("b") });
    like(await phone.next(reply), { type: "error", status: 400, poll: "p" });
    phone.send({ type: "vote", poll: "p", option: 0, voter: voter("a") });
    like(await phone.next(reply), { type: "voted" });
    phone.close();
  });

  test("a socket that sends too much is slowed down", async () => {
    const phone = await socket(newCode());
    for (let index = 0; index < 20; index += 1) phone.send("not json");
    for (let index = 0; index < 20; index += 1) {
      like(await phone.next(reply), { status: 400 });
    }
    phone.send("not json");
    like(await phone.next(reply), { status: 429 });
    phone.close();
  });

  test("a game takes 500 players", async () => {
    const code = newCode();
    const hex = (number) => number.toString(16).padStart(32, "0");
    for (let start = 0; start < 500; start += 50) {
      const batch = [];
      for (let index = start; index < start + 50; index += 1) {
        batch.push(joinGame(code, hex(index), `Player ${index}`));
      }
      for (const response of await Promise.all(batch)) assert.equal(response.status, 200);
    }
    const full = await joinGame(code, hex(500), "Late");
    assert.equal(full.status, 503);
    assert.equal((await full.json()).error, "the game is full");
    // A player already in may still rename.
    assert.equal((await joinGame(code, hex(0), "First")).status, 200);
  });
});
