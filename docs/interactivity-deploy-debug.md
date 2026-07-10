# Debug log: bringing the interactivity stack live

A record of the issues hit while deploying the comments / reactions /
contact layer (`blog-api` + Zola frontend) to the live server, and how
each was diagnosed and fixed. Read it as a runbook — the same failure
modes will recur on the next server or the next feature.

**Target:** `edwardhong.net` → Linode `172.104.62.92`, Caddy in front of
static files at `/var/www/blog` and (new) a reverse proxy to `blog-api`
on `127.0.0.1:8787`.

## The one-line mental model

There are **two independent deploys**, and every bug below came from
conflating them:

1. **Static site** — `blog/` templates + `static/js/interact.js`, built
   by Zola and rsync'd to `/var/www/blog`. Shipped by `mdpub` (or a raw
   `zola build` + `rsync`).
2. **Backend** — the `blog-api` binary + systemd unit + the Caddy
   `/api/*` route. Shipped by `blog-api/deploy.sh` **plus** manual Caddy
   config. `deploy.sh` does *not* touch Caddy.

A change to one is invisible until *that* deploy runs. Neither is
automatic.

---

## Symptom 1 — comment area missing on the live site

**Report:** "I don't see the comment area on the website."

**Diagnosis:** the comment form is static HTML baked into `page.html`, so
it should render even with the API down. Its absence meant the server was
serving pre-change HTML.

```bash
# local build HAS it:
grep -rl 'class="interact"' blog/public/blog/*/index.html   # 3 pages ✓
# live site does NOT:
curl -s https://edwardhong.net/blog/<slug>/ | grep -o 'class="interact"'   # (empty)
curl -s -o /dev/null -w '%{http_code}\n' https://edwardhong.net/js/interact.js   # 404
```

**Root cause:** the new templates/JS existed only in the local
`blog/public/`. `mdpub` rebuilds the whole site on *any* publish, but it
was never re-run after the template change.

**Fix:** force a publish of any article — it rebuilds every page + copies
`static/` and rsyncs:

```bash
mdpub publish Day3/loop-engineering.md --force
```

`--force` only overrides the "nothing changed" skip; it does **not** alter
the article's date/slug/content. Verified with `js:200` +
`class="interact"` count `1`.

> **Gotcha:** template/CSS/JS changes aren't tied to any article, so
> there's no content change to trigger a deploy. Either force-publish an
> article, or run the raw equivalent: `cd blog && zola build --base-url
> https://edwardhong.net && rsync -az --delete public/
> deploy@172.104.62.92:/var/www/blog/`. Hard-refresh (Cmd-Shift-R) after,
> since JS/CSS are cached.

---

## Symptom 2 — `/api/*` returns 404

**Key diagnostic — 404 vs 502:**

- **404** → Caddy never routed `/api/*`; it fell through to `file_server`,
  which returns its own 404. A *routing* problem.
- **502** → Caddy reached `blog-api` but the service is down. A *service*
  problem.

It was **404**, so the service wasn't even the suspect yet — Caddy was.
The decisive split is to test the service directly on loopback (bypassing
Caddy) vs. through the domain:

```bash
# on the server — is blog-api itself alive?
curl -s -o /dev/null -w 'loopback: %{http_code}\n' 'http://127.0.0.1:8787/api/comments?slug=x'
# through Caddy
curl -s -w '%{http_code}\n' 'https://edwardhong.net/api/comments?slug=x'
```

`loopback: 200` + external `404` = the service is healthy and Caddy is the
only thing wrong. (See Symptom 4 for the actual Caddy bug.)

---

## Symptom 3 — `deploy.sh` prompts for a password

**Report:** "it asks me to provide a password which I don't have… or does
it need `BLOG_API_ADMIN_TOKEN`?"

**Root cause:** *not* SSH (key auth works — that's why `mdpub` rsync is
passwordless) and *not* the admin token (that never appears in
`deploy.sh`). It was a **remote `sudo` prompt**. `deploy.sh` runs `sudo mv`
into `/usr/local/bin`, `sudo systemctl restart`, etc. over SSH, and the
`deploy` user (created with `--disabled-password`) has no password to give.

**Fix:** grant the `deploy` user *scoped* passwordless sudo for exactly the
commands `deploy.sh` runs. On the server as root:

```bash
tee /etc/sudoers.d/blog-api-deploy >/dev/null <<'EOF'
deploy ALL=(root) NOPASSWD: /usr/bin/mv /tmp/blog-api.new /usr/local/bin/blog-api, \
  /usr/bin/chmod 755 /usr/local/bin/blog-api, \
  /usr/bin/mkdir -p /var/lib/blog-api/migrations, \
  /usr/bin/cp -r /tmp/blog-api-migrations/. /var/lib/blog-api/migrations/, \
  /usr/local/bin/blog-api --migrate-only, \
  /usr/bin/systemctl restart blog-api
EOF
chmod 440 /etc/sudoers.d/blog-api-deploy
visudo -c   # must say "parsed OK"
```

> **Gotcha:** sudoers command matching is character-for-character against
> the command line `deploy.sh` sends. If it still prompts, reconcile the
> binary paths (`/usr/bin/mv` vs `/bin/mv`) and the exact `cp` arguments.

---

## Symptom 4 — still 404 after `deploy.sh` succeeded

**Root cause:** `deploy.sh` ships and restarts the *binary only*. It never
configures Caddy. So the service came up healthy on loopback, but `/api/*`
still 404'd because the Caddy route was broken.

Inspecting the Caddyfile revealed the real defect — the `handle /api/*`
block was placed **outside** the site block:

```
edwardhong.net, www.edwardhong.net {
    root * /var/www/blog
    file_server
    encode gzip
}                      # ← site block CLOSES here
handle /api/* {        # ← now a bogus second "site", never applies
    reverse_proxy 127.0.0.1:8787
}
```

Two bugs at once: (a) the block is outside the site block, so it does
nothing; and even if it were inside, (b) it sat *after* `file_server`,
which matches every path first (Caddy matchers are order-sensitive).

**Fix:** move both routes *inside* the site block using mutually-exclusive
`handle` blocks — `/api/*` peeled off before the static catch-all:

```
edwardhong.net, www.edwardhong.net {
    encode gzip

    handle /api/* {
        request_body {
            max_size 16KB
        }
        reverse_proxy 127.0.0.1:8787
    }

    handle {
        root * /var/www/blog
        file_server
    }
}
```

```bash
caddy validate --config /etc/caddy/Caddyfile   # "Valid configuration"
systemctl reload caddy
```

Verified: `curl https://edwardhong.net/api/comments?slug=<slug>` → **200**
`[]`. Stack fully live.

---

## Post-fix checklist

- Static: `curl -sw '%{http_code}' https://edwardhong.net/js/interact.js` → `200`
- API: `curl -sw '%{http_code}' 'https://edwardhong.net/api/comments?slug=x'` → `200`
- Service: `systemctl status blog-api` → `active (running)`
- Caddy: `caddy validate` → valid, `handle /api/*` **inside** the site
  block and **above** the static `handle`.

## Things that look broken but aren't

- **A submitted comment doesn't appear.** By design — comments post as
  `pending` and require admin approval (`docs/tutorial.md` §6). Reactions
  and the contact form are immediate.
- **Rate limit resets on restart.** In-memory by design; documented as an
  accepted v1 limitation.

## Reusable lessons

1. **Two deploys, always.** Frontend change → redeploy the static site.
   Backend change → `deploy.sh` **and** check Caddy.
2. **404 vs 502 tells you which layer failed** before you touch anything.
3. **Loopback-vs-domain curl** isolates service health from proxy routing
   in one step.
4. **Caddy is order-sensitive and block-scoped** — a route must be *inside*
   the site block and *before* any catch-all like `file_server`. Use
   `handle` blocks to make the routing mutually exclusive and unambiguous.
5. **"Password prompt" from an SSH-key-authed deploy is almost always
   remote `sudo`,** not SSH and not an app secret.
