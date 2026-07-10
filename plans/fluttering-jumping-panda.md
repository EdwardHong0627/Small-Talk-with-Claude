# Reader interactivity: comments, reactions, contact form

## Context

The blog (Zola + `mdpub`) is currently a pure static pipeline: markdown →
Zola build → `rsync -az --delete blog/public/ deploy@server:/var/www/blog/`.
`mdpub` never talks to a database or an API by design (its own tutorial
says so), and the site has zero client-side JavaScript today. To let
readers comment, react, and reach out, we need a small stateful backend
somewhere — the static pipeline has nowhere to put that state.

Goal: add comments, reactions/likes, and a contact form, without turning
`mdpub` or the static-site deploy model into something they're not. The
new interactive layer is a **separate, additive system** that lives
alongside the static site rather than replacing any part of it.

## Deliberate scope boundaries (read this before objecting to "missing" features)

- No outbound email (contact messages are stored + readable via an admin
  endpoint, not emailed).
- No CAPTCHA (honeypot + rate limiting + minimum content length is the v1
  spam defense; captcha is a real dependency/UX cost — revisit only if
  spam actually becomes a problem).
- No comment editing/deletion by the original author (no accounts, no
  auth for commenters — v1 is anonymous, moderator-approved only).
- No admin UI — moderation is token-gated JSON endpoints, used via `curl`
  or a REST client. A small `admin.html` is a natural v2, not v1.
- `mdpub` itself is **not modified**. It stays a pure content publisher.

## Architecture

```
reader's browser
  │  same-origin fetch("/api/...")
  ▼
Caddy (existing, auto-HTTPS)
  ├─ static files  →  /var/www/blog        (unchanged: mdpub's rsync target)
  └─ reverse_proxy /api/*  →  127.0.0.1:8787
                                  │
                            blog-api (new Rust service, systemd-managed)
                                  │
                            SQLite at /var/lib/blog-api/blog.db
                            (OUTSIDE /var/www/blog — mdpub's `rsync --delete`
                             must never see or touch this path)
```

Same-origin proxying means **no CORS handling needed in prod** and no new
firewall port (blog-api binds `127.0.0.1` only).

## 1. New crate: `blog-api/`

Sibling directory to `mdpub/`, own `Cargo.toml`/`Cargo.lock`, **not** a
Cargo workspace with `mdpub` — keeps `mdpub`'s existing build/lockfile
untouched (its `Cargo.lock` is intentionally tracked per `.gitignore`'s
comment).

Stack: `axum` + `tokio` + `rusqlite` with the **`bundled` feature**
(compiles SQLite in — no system libsqlite needed, and it's what makes
static musl cross-builds work, see §4). `Mutex<Connection>` — a single
file-backed SQLite DB under the traffic this blog gets doesn't need a
pool; handlers may block the runtime briefly on DB calls, which is
acceptable at this scale. Open the DB with `PRAGMA journal_mode=WAL` so
reads never stall behind writes. `tracing` for systemd-journal-friendly
logs. No `sqlx` (avoids compile-time DB/build requirements for no real
benefit here).

**Client IP extraction (load-bearing):** blog-api only ever sees
connections from Caddy on loopback, so the TCP peer address is always
`127.0.0.1`. Every IP-keyed defense below must instead take the client
IP from the `X-Forwarded-For` header (Caddy sets it), and trust that
header **only when the peer address is loopback** (direct local dev
requests fall back to the peer address). Put this in one helper
(`client_ip(...)` in `ratelimit.rs` or a small `ip.rs`) used by both the
rate limiter and the reaction cap.

Module layout:

```
blog-api/
  Cargo.toml
  src/
    main.rs        # wiring: load config, open DB, build Router, tokio::main
    config.rs       # env vars: BLOG_API_DB_PATH, BLOG_API_ADMIN_TOKEN,
                     #   BLOG_API_BIND_ADDR (default 127.0.0.1:8787),
                     #   BLOG_API_DEV_CORS_ORIGIN (optional, dev only)
    db.rs           # connection open + migration runner (applies
                     #   migrations/*.sql in order, tracked in a
                     #   schema_version table — safe to re-run)
    models.rs        # Comment, Reaction, ContactMessage structs + row mapping
    ratelimit.rs      # tower layer, keyed by (client_ip, route); client_ip
                     #   comes from X-Forwarded-For (see §1 note), NOT the
                     #   socket peer, which is always 127.0.0.1 behind Caddy
    auth.rs           # bearer-token middleware guarding /api/admin/*;
                     #   constant-time token comparison (e.g. subtle crate)
    routes/
      mod.rs          # assembles the Router, mounts sub-routers + layers
      comments.rs      # GET/POST /api/comments
      reactions.rs      # GET/POST /api/reactions
      contact.rs        # POST /api/contact
      admin.rs           # GET/POST under /api/admin/* (bearer-token gated)
  migrations/
    0001_init.sql     # comments, reactions, contact_messages, schema_version
  tests/
    comments.rs       # tower::ServiceExt::oneshot against rusqlite ":memory:"
    reactions.rs
    contact.rs
    admin.rs
  deploy.sh           # see §4
  deploy/
    blog-api.service   # systemd unit template (checked in, no secrets)
    caddy-snippet.conf   # reverse_proxy block to paste into the server's Caddyfile
```

### Endpoints

| Method | Path | Notes |
|---|---|---|
| `GET` | `/api/comments?slug=<slug>` | approved comments only |
| `POST` | `/api/comments` | `{slug, author, body, hp}` → inserted as **pending**; `hp` is the honeypot field, must be empty |
| `GET` | `/api/reactions?slug=<slug>` | counts per kind |
| `POST` | `/api/reactions` | `{slug, kind, client_id}` → increments; `client_id` is a random ID the frontend generates once and stores in `localStorage` |
| `POST` | `/api/contact` | `{name, email, message, hp}` → stored, not emailed |
| `GET` | `/api/admin/comments/pending` | bearer-token gated |
| `POST` | `/api/admin/comments/:id/approve` | bearer-token gated |
| `DELETE` | `/api/admin/comments/:id` | bearer-token gated |
| `GET` | `/api/admin/contact` | bearer-token gated, lists stored messages |

### Anti-abuse (v1)

- Honeypot hidden field (`hp`) on both the comment and contact forms —
  any non-empty value silently 200s without persisting (don't tip off
  bots).
- Min/max length checks on `author`/`body`/`message`.
- Rate limiter keyed by **(IP, route)**, not global — so a burst against
  `/api/contact` can't lock out `/api/comments`. The IP is the
  `X-Forwarded-For` value per §1 — keying by socket peer would collapse
  every reader into `127.0.0.1` and let one client exhaust the bucket for
  everyone. In-memory (e.g. `tower_governor` or hand-rolled token
  bucket); resets on `systemctl restart blog-api` — document this as an
  accepted limitation, not a real defense against a determined attacker.
- Reaction dedup is best-effort: client-side `localStorage` ID *plus* a
  server-side per-(IP, slug, kind) daily cap, since the client ID alone
  is trivially bypassable (incognito/clear storage).
- Pending comments that are never moderated just accumulate — acceptable
  at this scale; the admin can bulk-review via
  `GET /api/admin/comments/pending`. No auto-expiry in v1.
- Caddy also gets a request body size cap (see §3) so oversized POSTs
  aren't a cheap DoS vector before they even reach blog-api.

## 2. Database

`migrations/0001_init.sql`: `comments` (id, slug, author, body, status
[pending/approved], created_at), `reactions` (id, slug, kind, client_id,
ip, created_at — used for the per-IP+slug+kind daily cap), `contact_messages`
(id, name, email, message, created_at), `schema_version` (single-row
tracker so `db.rs`'s migration runner is idempotent and `deploy.sh` can
re-run safely).

DB file: `/var/lib/blog-api/blog.db` on the server (systemd
`StateDirectory`, see §3) — **never** under `/var/www/blog`.

**Backups:** this DB is the only non-regenerable state in the whole
architecture (everything else can be rebuilt from markdown). v1 story: a
nightly cron on the server runs `sqlite3 /var/lib/blog-api/blog.db
".backup /var/lib/blog-api/backup/blog-$(date +%u).db"` (7-day rotation
by weekday), and `deploy.sh` gets a `pull-backup` mode that scp's the
latest backup to the local machine. Litestream is the upgrade path if
this ever matters more.

## 3. Server provisioning (document in `docs/server-setup.md`, new section)

- No manual user or directory creation: the systemd unit uses
  `DynamicUser=yes` + `StateDirectory=blog-api`, which auto-provisions a
  locked-down service user and `/var/lib/blog-api` (passed to the app as
  `$STATE_DIRECTORY`) — less to document, better sandboxing than a
  hand-made account.
- Install `deploy/blog-api.service` to
  `/etc/systemd/system/blog-api.service`
  (`EnvironmentFile=/etc/blog-api.env` for the admin token — that file is
  created manually on the server, never committed), `Restart=on-failure`,
  binds `127.0.0.1:8787`.
- Add `deploy/caddy-snippet.conf`'s `handle /api/*` block into the
  existing site block in `/etc/caddy/Caddyfile`, **above** the
  `file_server` catch-all (Caddyfile matchers are order-sensitive — a
  `handle` after `file_server` never fires). Include a
  `request_body { max_size 16KB }`-style cap in that block.
  `caddy validate` before `systemctl reload caddy`.
- No `ufw` change needed — blog-api binds loopback only, reached solely
  through Caddy's already-open 443.

## 4. Deploy (kept separate from `mdpub`)

`blog-api/deploy.sh`. **A plain local `cargo build --release` cannot
work here**: the dev machine is an Apple Silicon Mac and the server is
x86_64 Linux — the scp'd binary would be the wrong OS and architecture.
Instead the script cross-compiles a static Linux binary:

```
cargo zigbuild --release --target x86_64-unknown-linux-musl
```

(one-time setup: `brew install zig`, `cargo install cargo-zigbuild`,
`rustup target add x86_64-unknown-linux-musl`; `rusqlite`'s `bundled`
feature from §1 is what makes the musl build fully static with no
server-side library dependencies). Building on the server instead is
explicitly rejected: compiling the axum/tokio stack on a 1GB Nanode
will likely OOM.

Then: `scp` the binary + `migrations/` to the server → `ssh` to run
migrations (`db.rs`'s runner, invoked via a `--migrate-only` CLI flag on
the same binary) and `systemctl restart blog-api`. This mirrors
`mdpub`'s existing pattern of "trust SSH-key access, shell out to real
tools" without adding any of this to `mdpub` itself, per the boundary
stated in `docs/tutorial.md`'s mental-model section.

## 5. Frontend (Zola side)

- **`blog/static/js/interact.js`** (first JS in the repo, vanilla, no
  bundler): API base resolution —
  `window.location.port === '1111' ? 'http://127.0.0.1:8787' : ''` (1111
  is Zola's dev-serve port used by `mdpub preview`; empty string means
  same-origin relative `/api/...` in prod). Functions: load + render
  comments for a slug, submit a comment (disable form + show "pending
  moderation" message on success), load + render reaction counts, submit
  a reaction (optimistic increment, generates/reuses a `localStorage`
  client id), submit the contact form (inline success/error, no reload —
  there's no server-rendered response page since the site is static).
  **XSS rule (non-negotiable):** comment authors/bodies are hostile
  user-generated content — all rendering goes through
  `document.createElement` + `textContent`, never `innerHTML` or
  string-concatenated HTML. One `innerHTML` on a comment body is stored
  XSS against every reader.
- **`blog/templates/base.html`**: add a `<script defer src="{{ get_url(path='js/interact.js') }}"></script>`
  before `</body>`; add a `contact` link to the existing
  `<nav class="side-nav">` (alongside home/archive/tags); small CSS
  additions to the existing inline `<style>` block for comment
  list/form and reaction buttons (matches the current convention — all
  CSS lives inline in `base.html`, no new stylesheet file).
- **`blog/templates/page.html`**: append a
  `<section class="interact" data-slug="{{ page.slug }}">` (reaction
  buttons + comment list/form) after `{{ page.content | safe }}` and
  before the existing `<p class="eof">` footer.
- **New contact page**: `blog/content/contact/index.md` (empty/minimal
  front matter) + `blog/templates/contact.html` (extends `base.html`,
  static HTML form with `name`/`email`/`message`/hidden honeypot field,
  submit handled by `interact.js`).

## 6. Docs

- `docs/server-setup.md`: new section for the provisioning steps in §3.
- `docs/tutorial.md`: short new section explaining the moderation
  workflow with concrete `curl` examples against the admin endpoints
  (list pending comments, approve one, view contact messages), and a
  one-line note that this layer is independent of `mdpub publish`.

## Verification

- `cargo test` inside `blog-api/` — unit tests for validation/rate-limit
  logic, and route-level integration tests via
  `tower::ServiceExt::oneshot` against an in-memory (`:memory:`) SQLite
  DB, covering: comment submit → pending → admin approve → appears in
  `GET /api/comments`; honeypot-filled submission is silently dropped;
  rate limit trips after N requests from one IP on one route without
  affecting a different route (tests set distinct `X-Forwarded-For`
  headers to simulate distinct clients — also proving the header, not
  the socket peer, is what's keyed on); reaction daily cap enforced per
  (IP, slug, kind).
- `cargo test` inside `mdpub/` — confirm it's unaffected (no changes
  expected there; this is a regression check on the existing 76 tests).
- Manual end-to-end: run `blog-api` locally (`BLOG_API_DB_PATH` pointed
  at a temp file), run `mdpub preview`, open a published post, submit a
  comment (verify it does *not* appear yet), hit the admin approve
  endpoint via `curl` with the bearer token, refresh and confirm it
  appears; click a reaction button and confirm the count increments and
  persists across reload; submit the contact form and confirm it shows
  up via `GET /api/admin/contact`. XSS probe: submit + approve a comment
  whose body is `<script>alert(1)</script><img src=x onerror=alert(2)>`
  and confirm it renders as literal text with no dialog/console error.
- Cross-compile check: `file target/x86_64-unknown-linux-musl/release/blog-api`
  should report a statically linked x86-64 Linux ELF before deploy.sh
  ever ships it.
- After deploying to the real server: `caddy validate` before reload,
  then `curl -I https://<domain>/api/comments?slug=<any>` to confirm the
  reverse proxy routes correctly and TLS still terminates normally for
  the rest of the site.

## Explicit follow-ups (not in this plan)

- Admin web UI (currently `curl`/REST client only).
- CAPTCHA, if spam becomes a real problem.
- Author-side comment edit/delete (would require introducing accounts).
- Email notifications for new comments/contact messages.
