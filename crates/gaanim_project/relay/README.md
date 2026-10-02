# Gaanim relay

Carries audience votes from phones to a [Gaanim](https://github.com/PaoloLupo/gaanim)
presentation. A poll authored with `scene.poll(...)` gives the scene a QR code
to this relay and the votes as live values; phones vote on the relay's page and
the presentation reads the counts while it runs.

It runs on Cloudflare Workers with a Durable Object per presentation, which
fits in the **Workers Free** plan. Phones and the presentation only make
outbound HTTPS connections, so it works on campus networks that isolate
devices and on mobile data.

Each phone holds one WebSocket: the relay pushes every question the moment
it opens and takes the votes on it. A connection costs one request, incoming
messages count 20 to 1, outgoing ones are free, and an idle session
hibernates, so a room of a few hundred phones uses a few thousand of the
plan's 100,000 daily requests. (Asking over HTTP every couple of seconds
instead would use them up with about forty phones in an hour.) Networks
that block WebSockets fall back to that HTTP polling automatically.

The presentation holds a WebSocket too, on which the relay pushes the
results as they change, so it never asks for them; a relay older than
version 6, or a network that blocks the socket, has it ask every second.

Storage is billed by the row, and the plan's tightest limit is 100,000 rows
written a day. A session reads its storage once when it wakes and then
answers from memory, so the results cost no reads; a vote writes one row, a quiz answer two, a reveal two
whatever the number of players, and the session's twelve-hour expiry moves
at most every half hour.

## Pages

- `/`: type the six-character code shown on the screen.
- `/s/<code>`: the voting page the QR code opens. Designed for a phone held in
  one hand: large answer tiles with a letter and a shape each, one vote per
  phone that can change while the question is open, the next question appears
  by itself, and it pauses while the phone is locked. Spanish or English by
  the phone's language; light or dark by its theme.

## Quizzes

A poll opened with a correct answer is a quiz, as in Kahoot. The first quiz
asks each phone for a nickname (2 to 20 letters, digits or spaces, unique in
the session). The phone counts down the quiz's time, measured by the relay's
clock so a phone cannot stretch it, and takes one answer. A correct answer
earns `points × (1 − elapsed / time / 2)`: all the points at once, half at the
last moment; a wrong one earns nothing. So that students sitting together
cannot copy, each phone shows a quiz's answers in its own order, colored by
place rather than like the screen, and once answered it only says so, the
same for everyone; its score does not change either. When the presentation
reveals the quiz, every phone shows the answer, and each player whether it
was right, the points it earned, its total and its place. The presentation
reads the leaderboard from `results`, can remove a player (who cannot join
again from that phone) and can reset the session before a new game. When the
questions are over, phones show the podium, and a goodbye once the
presentation ends; the next presentation on the session starts a new game.

A presentation that shows its audience opens a *lobby*: phones ask for the
nickname as soon as they open the page and wait in the room with it, and
`results` lists the players in the order they joined, so the presentation
can fill a waiting room before the first question.

## Deploy

With a free Cloudflare account, the button deploys it in a few clicks: it
copies the relay into a repository of yours and publishes it from there.

[![Deploy to Cloudflare](https://deploy.workers.cloudflare.com/button)](https://deploy.workers.cloudflare.com/?url=https://github.com/PaoloLupo/gaanim/tree/main/crates/gaanim_project/relay)

Or from this folder, which `gaanim relay init` writes, with Node.js 20+:

```sh
npm install
npx wrangler login
npm run deploy          # https://gaanim-relay.<account>.workers.dev
npm run deploy:test     # https://gaanim-relay-test.<account>.workers.dev
```

Then point Gaanim at it:

```sh
gaanim relay use https://gaanim-relay.<account>.workers.dev
```

or, for one project, in its `gaanim.toml`:

```toml
[polls]
relay = "https://gaanim-relay.<account>.workers.dev"
```

`GAANIM_POLL_RELAY` overrides both. `gaanim relay` shows the relay in use
and whether it speaks this Gaanim's protocol; a presentation warns when it
does not. To update a relay, write it again over its folder and deploy:

```sh
gaanim relay init --force gaanim-relay
cd gaanim-relay && npx wrangler deploy
```

## Develop

```sh
npm run dev             # http://localhost:8787
npm test                # end-to-end tests against a local relay
```

`GAANIM_POLL_RELAY=http://127.0.0.1:8787 gaanim --present <project>` presents
against the local relay; phones cannot reach it, so use a deployed relay in a
real room. (On Windows, `localhost` tries IPv6 first and each request waits
for it.)

## Limits

Anyone with a session's code can open its page, so a session bounds what a
phone can make it do:

- a game takes 500 players and a poll 1,000 phones; more get 503;
- a phone's WebSocket speaks for the first voter id it gives, sends at most
  20 messages each 10 seconds (more are refused with 429) and is closed past
  60;
- a question has 2 to 6 answers of up to 120 characters, a nickname 2 to 20
  letters, and a picture up to 600 KB of PNG, JPEG or WebP.

## API

The presentation (Gaanim) holds a random 64-hex-digit key; the first request
with it claims the session. Each poll has an id the presentation chooses
(`[a-z0-9-]`, up to 64), stable across presentations, so a poll keeps its
votes when the presentation comes back to it; reopening it with a different
question or answers starts it from zero. One poll is open at a time.

| Request | Who | Body / reply |
| --- | --- | --- |
| `PUT /s/<code>/poll` | presenter | `{id, question, options, multiple?, image?, correct?, time?, points?}` → `{id}`; opens that poll, a quiz with `correct` (an index, or a list for multiple choice; time 5–300 s, default 20; points 100–10000, default 1000); `image` names a picture stored with `PUT image` |
| `DELETE /s/<code>/poll` | presenter | closes the open poll |
| `POST /s/<code>/reveal` | presenter | `{id}`: shows a quiz's answer; it takes no more answers |
| `POST /s/<code>/kick` | presenter | `{name}`: removes a player and bans its phone |
| `POST /s/<code>/reset` | presenter | forgets every poll, vote, player and ban; the lobby stays |
| `POST /s/<code>/lobby` | presenter | `{open}`: phones ask for a nickname as soon as they open the page |
| `POST /s/<code>/ask` | presenter | `{label, required}`: joining also asks for this (a student code, a full name), at most 40 characters; `{label: ""}` stops asking |
| `GET /s/<code>/report` | presenter | the whole game to keep: `{game, started, ask, teams, polls: [{id, question, options, multiple, opened, correct, time, points, revealed, counts, respondents}], players: [{rank, name, extra, team, joined, score, correct, answered, streak, answers: {<id>: {options, elapsed?, points?, right?}}}]}`, questions in the order they opened; nothing is capped, and only here do players carry `extra` |
| `POST /s/<code>/teams` | presenter | `{names, colors, choose}`: play in 2 to 6 teams, `#rrggbb` colors; phones choose theirs with `choose`, or are dealt to the smallest; `{names: []}` plays alone |
| `PUT /s/<code>/image/<hash>` | presenter | a question's picture, its bytes with its `content-type`; `<hash>` is 16 hex digits; kept across games |
| `POST /s/<code>/stage` | presenter | `{stage}`: `"play"`, `"podium"` once the questions are over, `"end"` when the presentation ends; after `"end"`, the next `poll`, `lobby` or `stage` starts a new game, as `reset` does |
| `GET /s/<code>/results` | presenter | `{current, latest, game, connected, now, polls: {<id>: {open, counts, total, respondents, answers?, quiz?}}, players, playerCount, audience, teams}`; `connected` counts the phones' sockets, `answers` each player's `{name, options, elapsed, points, right}` (every poll here, the open one in pushes), `players` the leaderboard (top 100) with `{name, score, correct, answered, streak, avatar, team}`, `audience` the players in joining order (first 200), `teams` each team's `{score, players}` |
| `GET /s/<code>/presenter` | presenter | WebSocket that pushes `{type: "results", ...}`, the body of `GET results`, on connect and whenever it changes (at most every 250 ms) |
| `GET /s/<code>/ws` | phones | WebSocket, see below |
| `GET /s/<code>/poll?voter=<id>` | phones | `{lobby, stage, joined, open: false, podium?}` or `{lobby, stage, joined, open, id, question, options, chosen}`; `chosen` is that phone's vote or answer, or `null`; `joined` whether it plays in this game; `podium` the top three once the stage is not `"play"` |
| `POST /s/<code>/vote` | phones | `{poll, option, voter}`; 409 unless that poll is open (and, for a quiz, before its time is up and only once) |
| `POST /s/<code>/join` | phones | `{voter, name, avatar?, team?, extra?}` → `{player}`; 409 for a name in use, 400 `choose a team` when teams are chosen, 400 `missing extra` when the presentation asks for something required |
| `GET /s/<code>/image/<hash>` | phones | a question's picture |
| `GET /s/<code>/player?voter=<id>` | phones | `{player}`: name, score, place and last result, or `null`; the score leaves out a quiz not revealed yet |
| `GET /health` | anyone | `{relay: "gaanim", version: 12}` |

On the WebSocket the relay sends `{type: "poll", ...}` (the same body as
`GET /poll`, with `quiz: {time, deadline, now, revealed}` for a quiz) on
connect and whenever the question changes, each phone with its own `chosen`.
The phone does not keep its votes: the relay reports them, so a new game
starts clean everywhere. A phone introduces itself with
`{type: "hello", voter}` (answered `{type: "player", player}` and the
question with its `chosen`), joins a game
with `{type: "join", voter, name}` (answered `{type: "joined", player}`) and
votes with `{type: "vote", poll, option, voter}`, answered `{type: "voted",
poll, option}` or `{type: "error", status, error, poll}`. A reveal sends
each phone `{type: "result", poll, correct, option, points, player}`, and a
removed player gets `{type: "kicked"}`. `"ping"` is answered `"pong"` without
waking the session. The HTTP routes stay for networks that
block WebSockets.

Presenter requests send `Authorization: Bearer <key>`. Codes use
`A–Z` and `2–9` without `I` and `O`. A question has 2 to 6 answers.

## Privacy

Votes are anonymous: a voter is a random id the page keeps in the phone's
storage, with no accounts or cookies; a quiz asks only for a nickname, and
whatever the presentation asks with `ask`, which only the presenter's
`report` returns. Only the presentation's key can
read counts. A session and its votes are deleted twelve hours after its last
activity.
