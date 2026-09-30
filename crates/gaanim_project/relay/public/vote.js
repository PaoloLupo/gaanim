// Voting page for one presentation session, at /s/<code>.
//
// It polls the current question, shows its answers as large tiles and sends
// one vote per phone. The phone keeps a random voter id, so reloading the
// page keeps its vote; a vote can change while the question is open.
"use strict";

const REFRESH_MS = 1500;
const MAX_BACKOFF_MS = 10000;

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
  let timer = null;
  let failures = 0;

  function setStatus(message, emphasis) {
    status.replaceChildren();
    if (emphasis) {
      const strong = document.createElement("strong");
      strong.textContent = emphasis;
      status.append(strong);
    }
    if (message) status.append(message);
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

  function showQuestion(poll) {
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
      button.addEventListener("click", () => vote(poll, index, button));
      answers.append(item);
    });
    const saved = store.get(`gaanim-vote-${poll.id}`);
    markChosen(saved === null ? null : Number(saved));
    setStatus(saved === null ? "" : t("change"), saved === null ? "" : t("sent"));
    waiting.hidden = true;
    questionView.hidden = false;
  }

  async function vote(poll, index, button) {
    if (button.getAttribute("aria-busy") === "true") return;
    button.setAttribute("aria-busy", "true");
    setStatus(t("sending"));
    try {
      const response = await fetch(`/s/${code}/vote`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ poll: poll.id, option: index, voter }),
      });
      if (response.status === 409) {
        setStatus(t("closed"));
        showWaiting();
        return;
      }
      if (!response.ok) throw new Error(String(response.status));
      store.set(`gaanim-vote-${poll.id}`, String(index));
      markChosen(index);
      setStatus(t("change"), t("sent"));
      navigator.vibrate?.(18);
    } catch {
      setStatus(t("failed"));
    } finally {
      button.removeAttribute("aria-busy");
    }
  }

  function setConnection(live) {
    offline.hidden = live;
    dot.dataset.state = live ? "live" : "offline";
  }

  async function refresh() {
    clearTimeout(timer);
    try {
      const response = await fetch(`/s/${code}/poll`, { cache: "no-store" });
      if (!response.ok) throw new Error(String(response.status));
      const poll = await response.json();
      failures = 0;
      setConnection(true);
      if (poll.open) showQuestion(poll);
      else showWaiting();
    } catch {
      failures += 1;
      setConnection(false);
    }
    // Back off while offline so a dead network does not drain the battery.
    const wait = Math.min(REFRESH_MS * 2 ** Math.max(0, failures - 1), MAX_BACKOFF_MS);
    if (!document.hidden) timer = setTimeout(refresh, wait);
  }

  // A phone in a pocket stops asking; it catches up the moment it is back.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) clearTimeout(timer);
    else refresh();
  });
  window.addEventListener("online", refresh);

  refresh();
});
