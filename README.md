# Small Talk with Claude — self-hosted blog + `mdpub`

A self-hosted blog at **https://edwardhong.net**, published from plain
markdown files with a single command. No Medium, no platform APIs, no
lock-in: a [Zola](https://www.getzola.org) static site served by
[Caddy](https://caddyserver.com) on a $5 Linode, and **`mdpub`** — a Rust
CLI that turns any markdown file in this repo into a live article.

```
mdpub publish my-article.md      →  https://edwardhong.net/blog/my-article/
```

## How it works

```
article.md ──▶ mdpub ──▶ blog/content/blog/<slug>/index.md   (frontmatter translated,
                 │                                            images colocated)
                 ├──▶ zola build --base-url https://edwardhong.net
                 ├──▶ rsync -az --delete blog/public/ deploy@server:/var/www/blog/
                 └──▶ .mdpub-state.json                      (publish tracking)
```

## Repo layout

| Path | What it is |
|---|---|
| `mdpub/` | The Rust CLI (`cargo install --path mdpub`) |
| `blog/` | Zola site: config, templates, imported articles |
| `blog-api/` | Reader-interactivity API (comments, reactions, contact) — separate service, own `deploy.sh` |
| `approve_comments.sh` | Approve **all** pending comments in one go |
| `docs/tutorial.md` | **Start here** — full walkthrough of writing & publishing |
| `docs/server-setup.md` | One-time Linode + Caddy + DNS setup guide |
| `mdpub.toml` | Deploy config (git-ignored — contains the server address) |
| `.mdpub-state.json` | What's published, with content hashes (git-ignored) |

## Command cheat sheet

| Command | What it does |
|---|---|
| `mdpub publish <file.md>` | Import → build → deploy → print the live URL |
| `mdpub publish <file.md> --dry-run` | Import + build only; deploys nothing, records nothing |
| `mdpub publish <file.md> --force` | Republish even if content is unchanged |
| `mdpub publish <file.md> --draft` | Deploy as a Zola draft (not rendered publicly) |
| `mdpub preview` | `zola serve` with live reload at http://127.0.0.1:1111 |
| `mdpub status` | Each tracked article: `published` / `changed since publish` |
| `mdpub unpublish <file.md>` | Remove the article from the site and redeploy |
| `mdpub init --server <ssh> --base-url <url>` | Create `mdpub.toml` on a new machine |

Exit code `2` from `publish` means "already published and unchanged" —
useful in scripts.

## Writing articles

No frontmatter required — the title comes from the first `# heading` and
the date defaults to the moment of first publish (kept stable across
republishes, so same-day posts sort by publish order). Optional YAML
frontmatter:

```yaml
---
title: Custom Title            # otherwise: first "# h1" in the file
tags: [mcp, api-design]        # or a comma-separated string
date: 2026-07-12               # otherwise: first publish time (stable on republish)
description: One-line summary  # used in listings and meta tags
canonical_url: https://…       # if cross-posted from elsewhere
draft: true                    # publish but don't render publicly
---
```

Local images just work: `![diagram](assets/diagram.png)` (path relative
to the `.md` file) is copied next to the page and deployed with it.
Editing an image counts as a content change.

## Comments: the pending → approved mechanism

Reader comments are served by `blog-api` (a small axum + SQLite service
behind Caddy at `/api/*`, deployed separately via `blog-api/deploy.sh` —
`mdpub` knows nothing about it). Comments are **moderated by default**:

```
reader submits ──▶ POST /api/comments ──▶ stored as status = 'pending'
                                            │  (invisible to readers —
                                            │   GET /api/comments returns
                                            │   approved only)
you approve  ──▶ POST /api/admin/comments/<id>/approve ──▶ 'approved', public
```

Moderation is done with `curl` + the admin bearer token (set in
`/etc/blog-api.env` on the server, never committed):

```bash
# list what's waiting
curl -s -H "Authorization: Bearer $TOKEN" https://edwardhong.net/api/admin/comments/pending

# approve one (id from the list)
curl -s -X POST -H "Authorization: Bearer $TOKEN" https://edwardhong.net/api/admin/comments/<id>/approve

# reject one (works on approved comments too)
curl -s -X DELETE -H "Authorization: Bearer $TOKEN" https://edwardhong.net/api/admin/comments/<id>

# or approve everything pending in one go
TOKEN=<admin token> ./approve_comments.sh
```

To skip moderation entirely, set `BLOG_API_AUTO_APPROVE=1` in
`/etc/blog-api.env` and restart the service — new comments then publish
immediately (spam included, until you `DELETE` it). Unset it to return
to moderated mode; already-approved comments stay public either way.

## Development

```bash
cd mdpub
cargo test          # 76 tests: unit + CLI integration (no network needed)
cargo install --path .

cd ../blog-api
cargo test          # route-level tests against in-memory SQLite
```

External commands (`zola`, `rsync`) sit behind a `Runner` trait and are
mocked in tests; integration tests use stub binaries. Requirements at
runtime: `zola` (`brew install zola`), `rsync`, and SSH-key access to the
server.
