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

## Pages

- `/`: type the six-character code shown on the screen.
- `/s/<code>`: the voting page the QR code opens. Designed for a phone held in
  one hand: large answer tiles with a letter and a shape each, one vote per
  phone that can change while the question is open, the next question appears
  by itself, and it pauses while the phone is locked. Spanish or English by
  the phone's language; light or dark by its theme.

## Deploy

Requires Node.js 20+ and a free Cloudflare account.

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

`GAANIM_POLL_RELAY` overrides both.

## Develop

```sh
npm run dev             # http://localhost:8787
npm test                # end-to-end tests against a local relay
```

`GAANIM_POLL_RELAY=http://localhost:8787 gaanim --present <project>` presents
against the local relay; phones cannot reach `localhost`, so use a deployed
relay in a real room.

## API

The presentation (Gaanim) holds a random 64-hex-digit key; the first request
with it claims the session. Each poll has an id the presentation chooses
(`[a-z0-9-]`, up to 64), stable across presentations, so a poll keeps its
votes when the presentation comes back to it; reopening it with a different
question or answers starts it from zero. One poll is open at a time.

| Request | Who | Body / reply |
| --- | --- | --- |
| `PUT /s/<code>/poll` | presenter | `{id, question, options}` → `{id}`; opens that poll |
| `DELETE /s/<code>/poll` | presenter | closes the open poll |
| `GET /s/<code>/results` | presenter | `{current, connected, polls: {<id>: {open, counts, total}}}`; `connected` counts the phones' sockets |
| `GET /s/<code>/ws` | phones | WebSocket, see below |
| `GET /s/<code>/poll` | phones | `{open: false}` or `{open, id, question, options}` |
| `POST /s/<code>/vote` | phones | `{poll, option, voter}`; 409 unless that poll is open |
| `GET /health` | anyone | `{relay: "gaanim", version: 3}` |

On the WebSocket the relay sends `{type: "poll", ...}` (the same body as
`GET /poll`) on connect and whenever the question changes. A phone votes
with `{type: "vote", poll, option, voter}` and gets `{type: "voted", poll,
option}` or `{type: "error", status, error, poll}`; `"ping"` is answered
`"pong"` without waking the session. The HTTP routes stay for networks that
block WebSockets.

Presenter requests send `Authorization: Bearer <key>`. Codes use
`A–Z` and `2–9` without `I` and `O`. A question has 2 to 6 answers.

## Privacy

Votes are anonymous: a voter is a random id the page keeps in the phone's
storage, with no names, accounts or cookies. Only the presentation's key can
read counts. A session and its votes are deleted twelve hours after its last
activity.
