// Voting page for one presentation session, at /s/<code>.
//
// It keeps one WebSocket to the session: the relay pushes each question as
// it opens or closes and takes the votes on the same connection. Networks
// that block WebSockets fall back to asking over HTTP every few seconds.
// The answers show as large tiles, one vote per phone; the phone keeps a
// random voter id, so reloading the page keeps its vote.
//
// A poll's vote can change while the question is open. A quiz asks for a
// nickname first, counts down its time, takes one answer, and when the
// presenter reveals it shows whether the answer was right, the points it
// earned and the player's place.
"use strict";

/** Keepalive, answered by the relay without waking the session. */
const PING_MS = 25000;
/** How often the HTTP fallback asks for the question. */
const POLL_MS = 3000;
const MAX_BACKOFF_MS = 15000;
/** Failed connections before assuming the network blocks WebSockets. */
const SOCKET_ATTEMPTS = 3;
/** How long a vote sent on the socket may wait for its answer. */
const VOTE_TIMEOUT_MS = 5000;
/** The timer turns red below this. */
const URGENT_MS = 5000;

// A shape per answer, so an answer is recognizable without telling its
// color apart; the letter matches the bars on the presentation screen.
const SHAPES = [
  '<path d="M12 3l9.5 17h-19z"/>',
  '<path d="M12 2l9 10-9 10-9-10z"/>',
  '<circle cx="12" cy="12" r="9.5"/>',
  '<rect x="3" y="3" width="18" height="18" rx="2"/>',
  '<path d="M12 2.5l2.9 6.1 6.6.8-4.9 4.6 1.3 6.6L12 17.3l-5.9 3.3 1.3-6.6L2.5 9.4l6.6-.8z"/>',
  '<path d="M7 3h10l5 9-5 9H7l-5-9z"/>',
];

const code = location.pathname.split("/")[2]?.toUpperCase() ?? "";
const store = {
  get(key) {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  },
  set(key, value) {
    try {
      localStorage.setItem(key, value);
    } catch {
      // Private mode: the vote still counts, it just is not remembered.
    }
  },
};

function voterId() {
  let id = store.get("gaanim-voter");
  if (!id || !/^[0-9a-f]{32}$/.test(id)) {
    id = [...crypto.getRandomValues(new Uint8Array(16))]
      .map((byte) => byte.toString(16).padStart(2, "0"))
      .join("");
    store.set("gaanim-voter", id);
  }
  return id;
}

document.addEventListener("DOMContentLoaded", () => {
  const voter = voterId();
  const $ = (id) => document.getElementById(id);
  const views = {
    waiting: $("waiting"),
    join: $("join-view"),
    question: $("question"),
    result: $("result-view"),
    kicked: $("kicked-view"),
  };
  const questionText = $("question-text");
  const answers = $("answers");
  const status = $("status");
  const offline = $("offline");
  const dot = $("dot");
  const me = $("me");
  const template = $("answer-template");
  const timer = $("timer");
  const timerFill = $("timer-fill");
  const timerText = $("timer-text");
  const joinForm = $("join-form");
  const nameInput = $("name");
  const nameError = $("name-error");

  $("session").textContent = code;
  $("session").parentElement.title = t("sessionTitle");
  document.title = `Gaanim · ${code}`;
  nameInput.value = store.get("gaanim-name") ?? "";

  /** The question the relay last reported. */
  let poll = { open: false };
  /** The question drawn as tiles. */
  let shown = null;
  let current = "waiting";
  let player = null;
  /** Whether the relay said who this phone is, so a missing player means
   * it has not joined yet. */
  let known = false;
  let kicked = false;
  let socket = null;
  let socketFailures = 0;
  let usePolling = false;
  let retry = null;
  let pollTimer = null;
  let pingTimer = null;
  let tick = null;
  let joining = false;
  /** The vote waiting for the relay's answer: {poll, option, button, timer}. */
  let pending = null;

  function view(name) {
    current = name;
    for (const [key, element] of Object.entries(views)) element.hidden = key !== name;
    if (name !== "question") stopTimer();
  }

  function setStatus(message, emphasis) {
    status.replaceChildren();
    if (emphasis) {
      const strong = document.createElement("strong");
      strong.textContent = emphasis;
      status.append(strong);
    }
    if (message) status.append(message);
  }

  function setConnection(live) {
    offline.hidden = live || kicked;
    dot.dataset.state = live ? "live" : "offline";
  }

  function setPlayer(value) {
    known = true;
    player = value;
    me.hidden = !player;
    if (player) me.textContent = `${player.name} · ${formatScore(player.score)}`;
  }

  function markChosen(index) {
    answers.toggleAttribute("data-voted", index !== null);
    for (const button of answers.querySelectorAll(".answer")) {
      button.setAttribute("aria-pressed", String(Number(button.dataset.index) === index));
    }
  }

  function lock(message) {
    answers.setAttribute("data-locked", "");
    for (const button of answers.querySelectorAll(".answer")) button.disabled = true;
    if (message) setStatus(message);
  }

  // --- What the relay reports ----------------------------------------------

  function show(next) {
    poll = next;
    if (kicked) return;
    if (!poll.open) {
      // A revealed result stays until the next question.
      if (current !== "result") {
        shown = null;
        view("waiting");
      }
      return;
    }
    if (poll.quiz?.revealed) {
      if (player?.last?.poll === poll.id) showResult({ ...player.last, player });
      else showResult({ poll: poll.id, correct: poll.quiz.correct, option: null, player: null });
      return;
    }
    if (poll.quiz && !player) {
      view(known ? "join" : "waiting");
      return;
    }
    drawQuestion();
  }

  function drawQuestion() {
    const quiz = poll.quiz;
    if (shown !== poll.id) {
      shown = poll.id;
      questionText.textContent = poll.question;
      answers.replaceChildren();
      answers.removeAttribute("data-locked");
      poll.options.forEach((option, index) => {
        const item = template.content.firstElementChild.cloneNode(true);
        const button = item.querySelector(".answer");
        button.dataset.index = String(index);
        button.querySelector(".shape").innerHTML =
          `<svg viewBox="0 0 24 24">${SHAPES[index % SHAPES.length]}</svg>`;
        button.querySelector(".label").textContent = option;
        button.querySelector(".letter").textContent = String.fromCharCode(65 + index);
        const id = poll.id;
        button.addEventListener("click", () => vote(id, index, button));
        answers.append(item);
      });
      const saved = store.get(`gaanim-vote-${poll.id}`);
      markChosen(saved === null ? null : Number(saved));
      if (quiz && saved !== null) lock(t("locked"));
      else setStatus(saved === null ? "" : t("change"), saved === null ? "" : t("sent"));
    }
    view("question");
    if (quiz) startTimer(quiz);
    else timer.hidden = true;
  }

  function startTimer(quiz) {
    stopTimer();
    timer.hidden = false;
    // The deadline is on the relay's clock: shift it to this phone's.
    const deadline = quiz.deadline - (quiz.now - Date.now());
    const total = quiz.time * 1000;
    const update = () => {
      const remaining = Math.max(0, deadline - Date.now());
      timerFill.style.transform = `scaleX(${remaining / total})`;
      timerText.textContent = `${Math.ceil(remaining / 1000)}${t("seconds")}`;
      timer.toggleAttribute("data-urgent", remaining < URGENT_MS);
      if (remaining === 0) {
        stopTimer();
        if (!answers.hasAttribute("data-locked")) lock(t("timeUp"));
      }
    };
    update();
    tick = setInterval(update, 100);
  }

  function stopTimer() {
    clearInterval(tick);
    tick = null;
  }

  function showResult({ correct, option, points = 0, player: result }) {
    if (result) setPlayer(result);
    const outcome = !result ? "answer" : option === null ? "missed" : option === correct ? "correct" : "wrong";
    const answer = poll.options?.[correct];
    views.result.dataset.outcome = outcome;
    $("verdict").textContent = { correct: "✓", wrong: "✗", missed: "–", answer: "✓" }[outcome];
    $("result-title").textContent =
      outcome === "answer" ? t("answerWas") : t(outcome);
    const pointsLine = $("result-points");
    const detail = $("result-detail");
    if (outcome === "answer") {
      pointsLine.textContent = answer ?? "";
      detail.textContent = "";
    } else {
      pointsLine.textContent = `+${formatScore(points)} ${t("points")}`;
      const place = result.rank ? `${t("place", result.rank, result.players)} · ` : "";
      const reminder = outcome === "correct" || answer === undefined ? "" : `${t("answerWas")}: ${answer}. `;
      detail.textContent = `${reminder}${place}${formatScore(result.score)} ${t("points")} ${t("total")}`;
    }
    shown = null;
    view("result");
    if (outcome === "correct") navigator.vibrate?.([30, 40, 30]);
  }

  function showKicked() {
    kicked = true;
    clearTimeout(retry);
    clearTimeout(pollTimer);
    socket?.close();
    setPlayer(null);
    offline.hidden = true;
    view("kicked");
  }

  // --- Joining a game ------------------------------------------------------

  function joined(value) {
    joining = false;
    nameError.textContent = "";
    setPlayer(value);
    store.set("gaanim-name", value.name);
    show(poll);
  }

  function joinFailed(statusCode) {
    joining = false;
    if (statusCode === 403) return showKicked();
    nameError.textContent = statusCode === 409 ? t("nameTaken") : t("nameInvalid");
  }

  joinForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const name = nameInput.value.trim();
    if (joining) return;
    joining = true;
    nameError.textContent = "";
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: "join", voter, name }));
      return;
    }
    try {
      const response = await fetch(`/s/${code}/join`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ voter, name }),
      });
      if (response.ok) joined((await response.json()).player);
      else joinFailed(response.status);
    } catch {
      joining = false;
      nameError.textContent = t("failed");
    }
  });

  // --- Votes --------------------------------------------------------------

  /** What a refused vote means, from the relay's status and error. */
  function refusal(statusCode, error) {
    if (statusCode === 401) return "join";
    if (statusCode === 403) return "kicked";
    if (error === "time is up") return "timeUp";
    if (error === "already answered") return "answered";
    if (statusCode === 409) return "closed";
    return "failed";
  }

  function settle(pollId, option, outcome) {
    if (pending && pending.poll === pollId) {
      clearTimeout(pending.timer);
      pending.button.removeAttribute("aria-busy");
      pending = null;
    }
    const quiz = poll.id === pollId && poll.quiz;
    switch (outcome) {
      case "voted":
        store.set(`gaanim-vote-${pollId}`, String(option));
        if (shown === pollId) markChosen(option);
        if (quiz) lock(t("locked"));
        else setStatus(t("change"), t("sent"));
        navigator.vibrate?.(18);
        break;
      case "timeUp":
        lock(t("timeUp"));
        break;
      case "answered":
        lock(t("locked"));
        break;
      case "join":
        setPlayer(null);
        view("join");
        break;
      case "kicked":
        showKicked();
        break;
      case "closed":
        setStatus(t("closed"));
        break;
      default:
        setStatus(t("failed"));
    }
  }

  async function voteOverHttp(pollId, option) {
    try {
      const response = await fetch(`/s/${code}/vote`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ poll: pollId, option, voter }),
      });
      if (response.ok) return settle(pollId, option, "voted");
      const { error } = await response.json().catch(() => ({}));
      settle(pollId, option, refusal(response.status, error));
    } catch {
      settle(pollId, option, "failed");
    }
  }

  function vote(pollId, option, button) {
    if (pending || answers.hasAttribute("data-locked")) return;
    button.setAttribute("aria-busy", "true");
    setStatus(t("sending"));
    pending = { poll: pollId, option, button, timer: null };
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: "vote", poll: pollId, option, voter }));
      // No answer on the socket: try once more over HTTP.
      pending.timer = setTimeout(() => voteOverHttp(pollId, option), VOTE_TIMEOUT_MS);
    } else {
      voteOverHttp(pollId, option);
    }
  }

  // --- WebSocket ----------------------------------------------------------

  function onMessage(message) {
    switch (message.type) {
      case "poll":
        return show(message);
      case "player":
        setPlayer(message.player);
        return show(poll);
      case "joined":
        return joined(message.player);
      case "voted":
        return settle(message.poll, message.option, "voted");
      case "result":
        if (message.player) setPlayer(message.player);
        return showResult(message);
      case "kicked":
        return showKicked();
      case "error":
        if (joining) return joinFailed(message.status);
        if (message.status === 403) return showKicked();
        if (pending?.poll === message.poll) {
          return settle(message.poll, pending.option, refusal(message.status, message.error));
        }
    }
  }

  function connect() {
    clearTimeout(retry);
    if (kicked || usePolling || (socket && socket.readyState <= WebSocket.OPEN)) return;
    const scheme = location.protocol === "https:" ? "wss" : "ws";
    let opened = false;
    let ws;
    try {
      ws = new WebSocket(`${scheme}://${location.host}/s/${code}/ws`);
    } catch {
      startPolling();
      return;
    }
    socket = ws;
    ws.addEventListener("open", () => {
      opened = true;
      socketFailures = 0;
      setConnection(true);
      // Introduce this phone, so a reveal reaches it and a player gets its
      // nickname and score back.
      ws.send(JSON.stringify({ type: "hello", voter }));
      pingTimer = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) ws.send("ping");
      }, PING_MS);
    });
    ws.addEventListener("message", (event) => {
      if (event.data === "pong") return;
      try {
        onMessage(JSON.parse(event.data));
      } catch {
        // A message this page does not understand.
      }
    });
    ws.addEventListener("close", () => {
      clearInterval(pingTimer);
      if (socket === ws) socket = null;
      if (kicked) return;
      setConnection(false);
      socketFailures += 1;
      if (!opened && socketFailures >= SOCKET_ATTEMPTS) {
        startPolling();
        return;
      }
      const wait = Math.min(1000 * 2 ** (socketFailures - 1), MAX_BACKOFF_MS);
      if (!document.hidden) retry = setTimeout(connect, wait);
    });
  }

  // --- HTTP fallback ------------------------------------------------------

  async function fetchPlayer() {
    const response = await fetch(`/s/${code}/player?voter=${voter}`, { cache: "no-store" });
    if (response.status === 403) return showKicked();
    if (response.ok) setPlayer((await response.json()).player);
  }

  async function startPolling() {
    usePolling = true;
    try {
      await fetchPlayer();
    } catch {
      // Retried with the next refresh.
    }
    refresh();
  }

  async function refresh() {
    clearTimeout(pollTimer);
    if (kicked) return;
    try {
      const response = await fetch(`/s/${code}/poll`, { cache: "no-store" });
      if (!response.ok) throw new Error(String(response.status));
      const next = await response.json();
      // A newly revealed quiz: fetch this player's result first.
      if (next.quiz?.revealed && player && player.last?.poll !== next.id) await fetchPlayer();
      show(next);
      setConnection(true);
    } catch {
      setConnection(false);
    }
    if (!document.hidden) pollTimer = setTimeout(refresh, POLL_MS);
  }

  // A phone in a pocket may lose its connection; it catches up as soon as
  // it is back, and the relay sends the current question on connect.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) {
      clearTimeout(pollTimer);
      return;
    }
    if (usePolling) refresh();
    else connect();
  });
  window.addEventListener("online", () => (usePolling ? refresh() : connect()));

  connect();
});
