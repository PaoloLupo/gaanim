# Gaanim poll relay

A Cloudflare Worker that carries audience votes from phones to a Gaanim
presentation. Each Gaanim user deploys their own; it fits in the Workers Free
plan (Durable Objects with SQLite storage).

## Deploy

Requires Node.js and a free Cloudflare account.

```sh
npx wrangler login
npx wrangler deploy
```

Wrangler prints the address, e.g. `https://gaanim-relay.<you>.workers.dev`.
Tell Gaanim to use it:

```sh
gaanim relay use https://gaanim-relay.<you>.workers.dev
```

Or, only for one project, in its `gaanim.toml`:

```toml
[polls]
relay = "https://gaanim-relay.<you>.workers.dev"
```

`GAANIM_POLL_RELAY` overrides both.

## Try it locally

```sh
npx wrangler dev
GAANIM_POLL_RELAY=http://localhost:8787 gaanim --present .
```

Phones cannot reach `localhost`; use a deployed relay in a real room.

## How it works

A presentation opens a session under a six-character code and a secret key
only it knows. The QR code points to `/s/<code>`, a page that shows the
current question and sends one anonymous vote per phone (a random id kept in
the phone's storage; a phone can change its vote while the question is open).
Only the key can open or close questions and read the counts. A session and
its votes are deleted twelve hours after its last activity.
