// Voting page for one presentation session, at /s/<code>.
//
// It keeps one WebSocket to the session: the relay pushes each question as
// it opens or closes and takes the votes on the same connection. Networks
// that block WebSockets fall back to asking over HTTP every few seconds.
// The answers show as large tiles, one vote per phone; the phone keeps a
// random voter id, so reloading the page keeps its vote, and a vote can
// change while the question is open.
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
  const waiting = $("waiting");
  const questionView = $("question");
  const questionText = $("question-text");
  const answers = $("answers");
  const status = $("status");
  const offline = $("offline");
  const dot = $("dot");
  const template = $("answer-template");

  $("session").textContent = code;
  $("session").parentElement.title = t("sessionTitle");
  document.title = `Gaanim · ${code}`;

  let shown = null;
  let socket = null;
  let socketFailures = 0;
  let usePolling = false;
  let retry = null;
  let pollTimer = null;
  let pingTimer = null;
  /** The vote waiting for the relay's answer: {poll, option, button, timer}. */
  let pending = null;

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
    offline.hidden = live;
    dot.dataset.state = live ? "live" : "offline";
  }

  function showWaiting() {
    shown = null;
    questionView.hidden = true;
    waiting.hidden = false;
  }

  function markChosen(index) {
    answers.toggleAttribute("data-voted", index !== null);
    for (const button of answers.querySelectorAll(".answer")) {
      button.setAttribute("aria-pressed", String(Number(button.dataset.index) === index));
    }
  }

  function show(poll) {
    if (!poll.open) {
      showWaiting();
      return;
    }
    if (shown === poll.id) return;
    shown = poll.id;
    questionText.textContent = poll.question;
    answers.replaceChildren();
    poll.options.forEach((option, index) => {
      const item = template.content.firstElementChild.cloneNode(true);
      const button = item.querySelector(".answer");
      button.dataset.index = String(index);
      button.querySelector(".shape").innerHTML =
        `<svg viewBox="0 0 24 24">${SHAPES[index % SHAPES.length]}</svg>`;
      button.querySelector(".label").textContent = option;
      button.querySelector(".letter").textContent = String.fromCharCode(65 + index);
      button.addEventListener("click", () => vote(poll.id, index, button));
      answers.append(item);
    });
    const saved = store.get(`gaanim-vote-${poll.id}`);
    markChosen(saved === null ? null : Number(saved));
    setStatus(saved === null ? "" : t("change"), saved === null ? "" : t("sent"));
    waiting.hidden = true;
    questionView.hidden = false;
  }

  // --- Votes --------------------------------------------------------------

  function settle(poll, option, outcome) {
    if (pending && pending.poll === poll) {
      clearTimeout(pending.timer);
      pending.button.removeAttribute("aria-busy");
      pending = null;
    }
    if (outcome === "voted") {
      store.set(`gaanim-vote-${poll}`, String(option));
      if (shown === poll) markChosen(option);
      setStatus(t("change"), t("sent"));
      navigator.vibrate?.(18);
    } else if (outcome === "closed") {
      setStatus(t("closed"));
      showWaiting();
    } else {
      setStatus(t("failed"));
    }
  }

  async function voteOverHttp(poll, option) {
    try {
      const response = await fetch(`/s/${code}/vote`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ poll, option, voter }),
      });
      settle(poll, option, response.ok ? "voted" : response.status === 409 ? "closed" : "failed");
    } catch {
      settle(poll, option, "failed");
    }
  }

  function vote(poll, option, button) {
    if (pending) return;
    button.setAttribute("aria-busy", "true");
    setStatus(t("sending"));
    pending = { poll, option, button, timer: null };
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: "vote", poll, option, voter }));
      // No answer on the socket: try once more over HTTP.
      pending.timer = setTimeout(() => voteOverHttp(poll, option), VOTE_TIMEOUT_MS);
    } else {
      voteOverHttp(poll, option);
    }
  }

  // --- WebSocket ----------------------------------------------------------

  function connect() {
    clearTimeout(retry);
    if (usePolling || (socket && socket.readyState <= WebSocket.OPEN)) return;
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
      pingTimer = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) ws.send("ping");
      }, PING_MS);
    });
    ws.addEventListener("message", (event) => {
      if (event.data === "pong") return;
      let message;
      try {
        message = JSON.parse(event.data);
      } catch {
        return;
      }
      if (message.type === "poll") show(message);
      else if (message.type === "voted") settle(message.poll, message.option, "voted");
      else if (message.type === "error" && pending?.poll === message.poll) {
        settle(message.poll, pending.option, message.status === 409 ? "closed" : "failed");
      }
    });
    ws.addEventListener("close", () => {
      clearInterval(pingTimer);
      if (socket === ws) socket = null;
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

  function startPolling() {
    usePolling = true;
    refresh();
  }

  async function refresh() {
    clearTimeout(pollTimer);
    try {
      const response = await fetch(`/s/${code}/poll`, { cache: "no-store" });
      if (!response.ok) throw new Error(String(response.status));
      show(await response.json());
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
