// Voting page for one presentation session, at /s/<code>.
//
// It keeps one WebSocket to the session: the relay pushes each question as
// it opens or closes and takes the votes on the same connection. Networks
// that block WebSockets fall back to asking over HTTP every few seconds.
// The answers show as large tiles, one vote per phone; the phone keeps a
// random voter id, and the relay tells it what it chose on each question,
// so reloading the page keeps its vote and a new game starts clean.
//
// A quiz shows its answers in an order and colors of each phone's own, and
// once answered only says so, until the presenter reveals the answer: a
// neighbor's phone tells nothing. A poll's vote can change while the
// question is open. A presentation that
// shows its audience asks for a nickname and a character as soon as the page
// opens, and the phone waits in the room with them. The phone remembers both
// for the next presentation on this relay. A quiz asks for a nickname first, counts down its time, takes one answer, and when the
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

/** A 32-bit hash of `text` (FNV-1a). */
function hash(text) {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return h >>> 0;
}

/** 0..count-1 in an order that depends on `seed` alone, so a phone shows a
 * question the same way after reloading. */
function shuffled(count, seed) {
  let state = hash(seed);
  const random = () => {
    // mulberry32
    state = (state + 0x6d2b79f5) >>> 0;
    let x = state;
    x = Math.imul(x ^ (x >>> 15), x | 1);
    x ^= x + Math.imul(x ^ (x >>> 7), x | 61);
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
  };
  const order = [...Array(count).keys()];
  for (let i = count - 1; i > 0; i -= 1) {
    const j = Math.floor(random() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }
  return order;
}

/** The answers of a choice: an index, or a list of them. */
function listOf(choice) {
  if (Array.isArray(choice)) return choice;
  return Number.isInteger(choice) ? [choice] : [];
}

/** Whether two choices pick the same answers. */
function sameChoice(a, b) {
  const x = [...listOf(a)].sort((m, n) => m - n);
  const y = [...listOf(b)].sort((m, n) => m - n);
  return x.length === y.length && x.every((value, index) => value === y[index]);
}

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

// Earlier versions kept each vote on the phone, which outlived a reset of
// the game; the relay reports it now.
try {
  for (const key of Object.keys(localStorage)) {
    if (key.startsWith("gaanim-vote-")) localStorage.removeItem(key);
  }
} catch {
  // No storage: nothing to clean.
}

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
    sent: $("sent-view"),
    result: $("result-view"),
    finale: $("finale-view"),
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

  const creator = $("creator");
  const stage = $("avatar-stage");
  const partRows = $("avatar-parts");

  $("session").textContent = code;
  $("session").parentElement.title = t("sessionTitle");
  document.title = `Gaanim · ${code}`;
  nameInput.value = store.get("gaanim-name") ?? "";

  /** The question the relay last reported. */
  let poll = { open: false };
  /** The question drawn as tiles, and the answer marked on them. */
  let shown = null;
  let shownChoice = null;
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
  /** Changing the character after joining, from the waiting room. */
  let editing = false;
  /** The part catalog, once loaded, and the character being made. */
  let catalog = null;
  let avatar = null;
  /** The vote waiting for the relay's answer: {poll, option, button, timer}. */
  let pending = null;
  /** The team picked on the join form, when players choose. */
  let pickedTeam = null;
  /** The answers marked on a multiple choice question, not sent yet. */
  let picked = new Set();
  const submit = $("submit");
  const questionImage = $("question-image");

  /** The game's teams, or null without teams. */
  const teams = () => poll.teams ?? null;

  /** Tint the page with this player's team. */
  function showTeam() {
    const team = teams() && player && Number.isInteger(player.team) ? player.team : null;
    const badge = $("wait-team");
    if (team === null) {
      document.body.style.removeProperty("--team");
      badge.hidden = true;
      return;
    }
    const color = teams().colors[team];
    document.body.style.setProperty("--team", color);
    badge.textContent = t("teamLabel", teams().names[team]);
    badge.hidden = false;
  }

  /** The team buttons of the join form, when players choose. */
  function drawTeamPicker() {
    const picker = $("team-picker");
    const game = teams();
    picker.hidden = !game?.choose;
    if (!game?.choose) return;
    if (pickedTeam === null && Number.isInteger(player?.team)) pickedTeam = player.team;
    const options = $("team-options");
    options.replaceChildren();
    game.names.forEach((name, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "team-option";
      button.style.setProperty("--team-color", game.colors[index]);
      button.textContent = name;
      button.setAttribute("aria-pressed", String(pickedTeam === index));
      // A player who answered keeps the team.
      button.disabled = Boolean(player?.answered) && Number.isInteger(player?.team) && player.team !== index;
      button.addEventListener("click", () => {
        pickedTeam = index;
        nameError.textContent = "";
        drawTeamPicker();
      });
      options.append(button);
    });
  }

  /** The join form, worded for a lobby or for a quiz. */
  function showJoin(lobby) {
    drawTeamPicker();
    $("join-title").textContent = t(lobby ? "lobbyTitle" : "gameTitle");
    $("join-hint").textContent = t(lobby ? "lobbyHint" : "gameHint");
    $("join-button").textContent = t(lobby ? "lobbyButton" : "gameButton");
    view("join");
  }

  /** Waiting for a question: in the room, with its character, once this
   * phone joined. `cheer` makes the character celebrate arriving. */
  function showWaiting(cheer = false) {
    const inside = poll.lobby && player;
    showTeam();
    $("wait-title").textContent = inside ? t("inTitle", player.name) : t("waitTitle");
    $("wait-hint").textContent = inside ? t("inHint") : t("waitHint");
    const stageElement = $("wait-avatar");
    const shown = inside && catalog && live(stageElement, player.avatar ?? avatar, player.name);
    stageElement.hidden = !shown;
    $("wait-pulse").hidden = Boolean(shown);
    $("edit-button").hidden = !shown;
    if (shown && cheer) shown.express("happy");
    view("waiting");
  }

  /** The living characters on this page, by the element that holds them. */
  const living = new Map();

  /** Show `character` alive in `container`, keeping the one already there
   * when it is the same. Returns it, to play expressions on, or null. */
  function live(container, character, name, size = 150) {
    if (!catalog || !isAvatar(catalog, character)) return null;
    const key = `${character}|${name}|${size}`;
    const current = living.get(container);
    if (current?.key === key) return current.avatar;
    current?.avatar.stop();
    const alive = animateAvatar(catalog, character, size, name, t("avatarLabel"));
    container.replaceChildren(alive.element);
    living.set(container, { key, avatar: alive });
    return alive;
  }

  // --- The character ------------------------------------------------------

  /** Draw the character being made, alive; it reacts when it changed. */
  function drawCreator(changed = false) {
    if (!catalog || !avatar) return;
    const alive = live(stage, avatar, nameInput.value.trim() || code);
    if (changed) alive?.express("surprised");
  }

  function setAvatar(value) {
    avatar = value;
    store.set("gaanim-avatar", JSON.stringify(value));
    drawCreator(true);
  }

  /** A row per part, stepping through its choices. */
  function buildParts() {
    const labels = ["partBodies", "partColors", "partEyes", "partMouths", "partExtras"];
    partRows.replaceChildren();
    AVATAR_PARTS.forEach((part, index) => {
      const row = $("part-template").content.firstElementChild.cloneNode(true);
      row.querySelector(".part-name").textContent = t(labels[index]);
      for (const button of row.querySelectorAll(".step")) {
        const step = Number(button.dataset.step);
        button.setAttribute("aria-label", `${t(step < 0 ? "previous" : "next")}: ${t(labels[index])}`);
        button.addEventListener("click", () => {
          const count = catalog[part].length;
          const next = [...avatar];
          next[index] = (next[index] + step + count) % count;
          setAvatar(next);
        });
      }
      partRows.append(row);
    });
  }

  /** A nickname made of an animal and an adjective. */
  function randomName() {
    const pick = (list) => list[Math.floor(Math.random() * list.length)];
    return `${pick(t("nameNouns"))} ${pick(t("nameAdjectives"))}`;
  }

  $("avatar-random").addEventListener("click", () => {
    setAvatar(randomAvatar(catalog));
    living.get(stage)?.avatar.express("happy");
  });
  $("name-random").addEventListener("click", () => {
    nameInput.value = randomName();
    nameError.textContent = "";
  });
  $("edit-button").addEventListener("click", () => {
    editing = true;
    if (player?.avatar && catalog && isAvatar(catalog, player.avatar)) avatar = player.avatar;
    nameInput.value = player?.name ?? nameInput.value;
    drawCreator();
    showJoin(true);
  });

  loadAvatarCatalog()
    .then((loaded) => {
      catalog = loaded;
      let saved = null;
      try {
        saved = JSON.parse(store.get("gaanim-avatar") ?? "null");
      } catch {
        // Not a character: make a new one.
      }
      avatar = isAvatar(catalog, saved) ? saved : randomAvatar(catalog);
      store.set("gaanim-avatar", JSON.stringify(avatar));
      buildParts();
      drawCreator();
      creator.hidden = false;
      if (current === "waiting") showWaiting();
      if (current === "result") showResultAvatar();
    })
    .catch(() => {
      // Without the catalog a phone joins with a nickname alone.
    });

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
    showTeam();
  }

  /** A player who must pick a team first: teams changed, or it never did. */
  function needsTeam() {
    return Boolean(teams()?.choose && known && player && !Number.isInteger(player.team));
  }

  function markChosen(choice) {
    const chosen = listOf(choice);
    answers.toggleAttribute("data-voted", chosen.length > 0);
    for (const button of answers.querySelectorAll(".answer")) {
      button.setAttribute("aria-pressed", String(chosen.includes(Number(button.dataset.index))));
    }
  }

  /** The send button of a multiple choice question. */
  function drawSubmit() {
    submit.hidden = !poll.multiple;
    submit.disabled = picked.size === 0 || answers.hasAttribute("data-locked");
    const sent = poll.chosen !== null && poll.chosen !== undefined;
    submit.textContent = t(sent && !poll.quiz ? "update" : "submit");
  }

  /** Mark this phone's choice as the relay reports it: after a reset or a
   * new game there is none, and a quiz takes an answer again. */
  function applyChoice(choice, quiz) {
    shownChoice = choice;
    picked = new Set(listOf(choice));
    markChosen(choice);
    if (quiz && choice !== null) return lock(t("locked"));
    answers.removeAttribute("data-locked");
    for (const button of answers.querySelectorAll(".answer")) button.disabled = false;
    drawSubmit();
    if (choice === null) setStatus("");
    else setStatus(t("change"), t("sent"));
  }

  function lock(message) {
    answers.setAttribute("data-locked", "");
    submit.disabled = true;
    for (const button of answers.querySelectorAll(".answer")) button.disabled = true;
    if (message) setStatus(message);
  }

  // --- What the relay reports ----------------------------------------------

  /** When a quiz's time is up, on this phone's clock. */
  function deadlineOf(quiz) {
    // The deadline is on the relay's clock, measured when it arrived.
    quiz.skew ??= quiz.now - Date.now();
    return quiz.deadline - quiz.skew;
  }

  function show(next) {
    poll = next;
    if (kicked) return;
    if (!poll.open && (poll.stage === "podium" || poll.stage === "end")) {
      shown = null;
      return showFinale();
    }
    if (needsTeam() && !(editing && current === "join")) {
      shown = null;
      return showJoin(poll.lobby);
    }
    if (!poll.open) {
      // A revealed result stays until the next question, and a character
      // being changed until it is saved.
      if (current === "result" || (editing && current === "join")) return;
      shown = null;
      if (poll.lobby && known && !player) showJoin(true);
      else showWaiting();
      return;
    }
    if (poll.quiz?.revealed) {
      if (player?.last?.poll === poll.id) showResult({ ...player.last, player });
      else showResult({ poll: poll.id, correct: poll.quiz.correct, option: null, player: null });
      return;
    }
    if (poll.quiz && !player) {
      if (known) showJoin(poll.lobby);
      else showWaiting();
      return;
    }
    if (poll.quiz && !pending) {
      if (listOf(poll.chosen).length > 0) return showSent(true);
      if (Date.now() >= deadlineOf(poll.quiz)) return showSent(false);
    }
    drawQuestion();
  }

  function drawQuestion() {
    const quiz = poll.quiz;
    const choice = listOf(poll.chosen).length > 0 ? poll.chosen : null;
    if (shown !== poll.id) {
      shown = poll.id;
      shownChoice = undefined;
      picked = new Set();
      questionText.textContent = poll.question;
      questionImage.hidden = !poll.image;
      if (poll.image) questionImage.src = `/s/${code}/image/${poll.image}`;
      else questionImage.removeAttribute("src");
      $("question-hint").hidden = !poll.multiple;
      answers.toggleAttribute("data-multiple", Boolean(poll.multiple));
      answers.replaceChildren();
      answers.removeAttribute("data-locked");
      // A poll matches the screen's letters and colors; a quiz is this
      // phone's own, colored by place, so neighbors cannot copy a tile.
      const order = quiz
        ? shuffled(poll.options.length, `${voter}:${poll.id}`)
        : poll.options.map((_, index) => index);
      order.forEach((index, place) => {
        const option = poll.options[index];
        const look = (quiz ? place : index) % SHAPES.length;
        const item = template.content.firstElementChild.cloneNode(true);
        const button = item.querySelector(".answer");
        button.dataset.index = String(index);
        button.dataset.look = String(look);
        button.querySelector(".shape").innerHTML = `<svg viewBox="0 0 24 24">${SHAPES[look]}</svg>`;
        button.querySelector(".label").textContent = option;
        const letter = button.querySelector(".letter");
        if (quiz) letter.remove();
        else letter.textContent = String.fromCharCode(65 + index);
        const id = poll.id;
        button.addEventListener("click", () => {
          if (!poll.multiple) return vote(id, index, button);
          // Several answers: mark them, then send.
          if (answers.hasAttribute("data-locked") || pending) return;
          if (picked.has(index)) picked.delete(index);
          else picked.add(index);
          markChosen([...picked]);
          drawSubmit();
        });
        answers.append(item);
      });
    }
    // A vote on its way is settled by the relay's answer to it.
    if (!pending && !(shownChoice !== undefined && sameChoice(choice, shownChoice))) {
      applyChoice(choice, quiz);
    }
    drawSubmit();
    view("question");
    if (quiz) startTimer(quiz);
    else timer.hidden = true;
  }

  function startTimer(quiz) {
    stopTimer();
    timer.hidden = false;
    const deadline = deadlineOf(quiz);
    const total = quiz.time * 1000;
    const update = () => {
      const remaining = Math.max(0, deadline - Date.now());
      timerFill.style.transform = `scaleX(${remaining / total})`;
      timerText.textContent = `${Math.ceil(remaining / 1000)}${t("seconds")}`;
      timer.toggleAttribute("data-urgent", remaining < URGENT_MS);
      if (remaining === 0) {
        stopTimer();
        // An answer on its way is settled by the relay.
        if (!pending) showSent(false);
      }
    };
    update();
    tick = setInterval(update, 100);
  }

  function stopTimer() {
    clearInterval(tick);
    tick = null;
  }

  /** A quiz answered (or not, when `answered` is false and its time is
   * up): the same words for everyone, whichever answer they chose. */
  let sentKey = null;
  function showSent(answered) {
    shown = null;
    const key = `${poll.id}|${answered}`;
    const phrases = t("sentPhrases");
    $("sent-title").textContent = t(answered ? "sentTitle" : "timeUp");
    $("sent-hint").textContent = answered
      ? phrases[hash(`${voter}:${poll.id}`) % phrases.length]
      : t("timeUpHint");
    const holder = $("sent-avatar");
    const mine = player && catalog && live(holder, player.avatar ?? avatar, player.name);
    holder.hidden = !mine;
    $("sent-pulse").hidden = Boolean(mine);
    if (current !== "sent" || sentKey !== key) {
      sentKey = key;
      view("sent");
      if (mine) mine.express(answered ? "surprised" : "sad");
    }
  }

  /** The character on the result, reacting to how the answer went. */
  let resultOutcome = null;
  function showResultAvatar() {
    const holder = $("result-avatar");
    const shown = player && catalog && live(holder, player.avatar ?? avatar, player.name);
    holder.hidden = !shown;
    const reaction = { correct: "happy", wrong: "sad", missed: "hurt" }[resultOutcome];
    if (shown && reaction) shown.express(reaction, { loop: reaction === "happy" });
  }

  function showResult({ correct, option, points = 0, player: result }) {
    if (result) setPlayer(result);
    const missed = listOf(option).length === 0;
    const outcome = !result ? "answer" : missed ? "missed" : sameChoice(option, correct) ? "correct" : "wrong";
    const named = listOf(correct).map((index) => poll.options?.[index]);
    const answer = named.length > 0 && named.every((name) => name !== undefined) ? named.join(", ") : undefined;
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
    resultOutcome = outcome;
    view("result");
    showResultAvatar();
    if (outcome === "correct") navigator.vibrate?.([30, 40, 30]);
  }

  /** The questions are over: the podium while the presentation shows it,
   * then a goodbye with this player's place. */
  function showFinale() {
    const ended = poll.stage === "end";
    $("finale-title").textContent = t(ended ? "endTitle" : "podiumTitle");
    $("finale-hint").textContent = t(ended ? "endHint" : "podiumHint");
    const podium = $("podium");
    const standings = poll.podium ?? [];
    const key = JSON.stringify(standings);
    if (podium.dataset.key !== key) {
      podium.dataset.key = key;
      for (const holder of podium.querySelectorAll(".podium-avatar")) {
        living.get(holder)?.avatar.stop();
        living.delete(holder);
      }
      podium.replaceChildren();
      // Second, first, third, as podiums stand.
      for (const rank of [1, 0, 2]) {
        const entry = standings[rank];
        if (!entry) continue;
        const item = $("podium-template").content.firstElementChild.cloneNode(true);
        item.dataset.rank = String(rank + 1);
        item.querySelector(".podium-name").textContent = entry.name;
        item.querySelector(".podium-score").textContent = `${formatScore(entry.score)} ${t("points")}`;
        item.querySelector(".podium-step").textContent = String(rank + 1);
        podium.append(item);
        const alive = live(item.querySelector(".podium-avatar"), entry.avatar, entry.name, 64);
        if (alive && rank === 0) alive.express("winner", { loop: true });
      }
    }
    podium.hidden = standings.length === 0;
    // In teams, the teams' standings, this player's team marked.
    const board = $("standings");
    const teamStandings = poll.standings ?? [];
    board.hidden = teamStandings.length === 0;
    board.replaceChildren(
      ...teamStandings.map((team, place) => {
        const item = document.createElement("li");
        item.className = "standing";
        item.style.setProperty("--team-color", team.color);
        item.toggleAttribute("data-mine", team.team === player?.team);
        item.innerHTML = `<span class="standing-place"></span><span class="standing-name"></span><span class="standing-score"></span>`;
        item.querySelector(".standing-place").textContent = String(place + 1);
        item.querySelector(".standing-name").textContent = team.name;
        item.querySelector(".standing-score").textContent = `${formatScore(team.score)} ${t("points")}`;
        return item;
      }),
    );
    if (teamStandings.length > 0 && !ended) {
      const [first, second] = teamStandings;
      $("finale-title").textContent =
        second && second.score === first.score ? t("teamTie") : t("teamWon", first.name);
    }
    const holder = $("finale-avatar");
    const mine = player && catalog && live(holder, player.avatar ?? avatar, player.name);
    holder.hidden = !mine;
    if (player?.rank) {
      $("finale-place").textContent = t("place", player.rank, player.players);
      $("finale-detail").textContent = `${formatScore(player.score)} ${t("points")} ${t("total")}`;
    } else {
      $("finale-place").textContent = "";
      $("finale-detail").textContent = "";
    }
    if (current !== "finale" || views.finale.dataset.stage !== poll.stage) {
      views.finale.dataset.stage = poll.stage;
      view("finale");
      if (mine && player?.rank) {
        const reaction = player.rank === 1 ? "winner" : player.rank <= 3 ? "happy" : "surprised";
        mine.express(reaction, { loop: player.rank <= 3 });
      }
    }
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
    editing = false;
    nameError.textContent = "";
    setPlayer(value);
    store.set("gaanim-name", value.name);
    show(poll);
    if (current === "waiting") showWaiting(true);
  }

  function joinFailed(statusCode, error) {
    joining = false;
    if (statusCode === 403) return showKicked();
    if (error === "choose a team") {
      nameError.textContent = t("teamRequired");
      return;
    }
    nameError.textContent = statusCode === 409 ? t("nameTaken") : t("nameInvalid");
  }

  joinForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    const name = nameInput.value.trim();
    if (joining) return;
    if (teams()?.choose && pickedTeam === null) {
      nameError.textContent = t("teamRequired");
      return;
    }
    joining = true;
    nameError.textContent = "";
    const request = {
      voter,
      name,
      ...(avatar && { avatar }),
      ...(teams()?.choose && { team: pickedTeam }),
    };
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(JSON.stringify({ type: "join", ...request }));
      return;
    }
    try {
      const response = await fetch(`/s/${code}/join`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(request),
      });
      if (response.ok) joined((await response.json()).player);
      else joinFailed(response.status, (await response.json().catch(() => ({}))).error);
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
        if (poll.id === pollId) poll.chosen = option;
        if (shown === pollId) {
          shownChoice = option;
          markChosen(option);
          drawSubmit();
        }
        navigator.vibrate?.(18);
        if (quiz) return showSent(true);
        setStatus(t("change"), t("sent"));
        break;
      case "timeUp":
        if (quiz) return showSent(false);
        lock(t("timeUp"));
        break;
      case "answered":
        if (quiz) return showSent(true);
        lock(t("locked"));
        break;
      case "join":
        setPlayer(null);
        showJoin(poll.lobby);
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

  submit.addEventListener("click", () => {
    if (picked.size === 0 || !poll.open) return;
    vote(poll.id, [...picked].sort((a, b) => a - b), submit);
  });

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
        if (joining) return joinFailed(message.status, message.error);
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
      const response = await fetch(`/s/${code}/poll?voter=${voter}`, { cache: "no-store" });
      if (!response.ok) throw new Error(String(response.status));
      const next = await response.json();
      // A new game forgot this player.
      if (player && next.joined === false) setPlayer(null);
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
