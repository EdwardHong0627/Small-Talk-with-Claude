# mdpub tutorial: from markdown file to live article

This is the full walkthrough of publishing with `mdpub`. It assumes the
server is already set up (if not, do [server-setup.md](server-setup.md)
first — it's a one-time job).

## 0. One-time setup on a new machine

```bash
brew install zola                      # static site generator (build happens locally)
cargo install --path mdpub             # from the repo root
mdpub init --server user@ip --base-url https://hostname.com
```

`mdpub init` writes `mdpub.toml` (git-ignored, since it names your
server). Everything below is run from anywhere inside the repo — `mdpub`
finds `mdpub.toml` by walking up parent directories, like git does.

## 1. Write an article

Create a markdown file anywhere in the repo, e.g. `XX/why-caching.md`:

```markdown
# Why Your Cache Invalidation Story Is Probably Fine

Everyone quotes the "two hard problems" joke. Here's the thing…

## The actual failure modes

- stale reads after write
- thundering herds

![cache flow](cache-flow.png)
```

That's a complete, publishable article:

- **Title** — taken from the first `# heading` (and not duplicated on the
  page; the template renders it).
- **Slug** — defaults to a kebab-case of the full title, e.g. `Why Your
  Cache Invalidation Story Is Probably Fine` becomes
  `why-your-cache-invalidation-story-is-probably-fine`. Override it with
  `slug:` frontmatter for a shorter URL.
- **Date** — defaults to the moment of first publish, with the time
  included, so posts published on the same day still sort correctly.
  Republishing does not change it.
- **Images** — `cache-flow.png` is resolved relative to the `.md` file,
  copied into the site next to the article, and deployed with it. Remote
  images (`https://…`) are left as-is. A typo'd image path aborts the
  publish before anything is built or deployed.

Want more control? Add YAML frontmatter at the very top:

```markdown
---
title: Why Your Cache Invalidation Story Is Probably Fine
slug: cache-invalidation-is-fine
tags: [caching, systems]
date: 2026-07-12
description: The two-hard-problems joke, taken seriously for once.
---
```

Unknown frontmatter keys are rejected (typo protection), tags can be a
list or `tags: caching, systems`, dates accept `YYYY-MM-DD` or
RFC 3339, and `slug:` must be lowercase kebab-case (letters, digits,
single hyphens — no spaces, capitals, or leading/trailing hyphen).

## 2. Preview locally

```bash
mdpub preview
```

Runs `zola serve` with live reload and opens http://127.0.0.1:1111
(`--no-open` to skip the browser). Note: preview shows articles already
imported into `blog/content/`. To see a *new* article there first, import
it without deploying:

```bash
mdpub publish Day2/why-caching.md --dry-run    # import + build, deploy nothing
mdpub preview
```

## 3. Publish

```bash
mdpub publish Day2/why-caching.md
```

```
  Title:  Why Your Cache Invalidation Story Is Probably Fine
  Slug:   why-your-cache-invalidation-story-is-probably-fine
  Date:   2026-07-12
  Tags:   caching, systems
  Images: 1
  Live:   https://example.com/blog/why-your-cache-invalidation-story-is-probably-fine/
```

Under the hood: the article is translated to a Zola page
(`blog/content/blog/<slug>/index.md` + colocated images), the whole site
is rebuilt, and `rsync --delete` mirrors it to the server. The homepage,
archive, tag pages, and the Atom feed (`/atom.xml`) all update in the
same deploy.

## 4. Iterate

Edit the file (or just the image — that counts too) and publish again:

```bash
mdpub publish Day2/why-caching.md
```

If nothing changed, `mdpub` tells you and exits with code `2` instead of
redeploying; `--force` overrides. To see everything at a glance:

```bash
mdpub status
```

```
Day1/mcp-vs-rest-design.md  [published]  https://example.com/blog/mcp-is-not-…/
Day2/why-caching.md         [changed since publish]  https://example.com/blog/why-…/
```

Renaming the title is safe: the article gets a new slug and the page at
the old URL is removed in the same deploy (no zombie pages) — but note
that the old URL then 404s, so retitle before sharing links, not after.

## 5. Drafts and taking things down

```bash
mdpub publish Day2/half-baked.md --draft    # deployed but not rendered publicly
mdpub unpublish Day2/regret.md              # removed from the site, redeployed
```

A draft becomes public by publishing again without `--draft`.

## Troubleshooting

| Symptom | Likely cause / fix |
|---|---|
| `no mdpub.toml found … run mdpub init` | You're outside the repo, or a fresh clone — run the `init` from step 0 |
| `no title: add title: frontmatter or start with # Title` | The article has neither — add one |
| `image "x.png" not found` | Path is relative to the `.md` file — check spelling/location |
| `slug "…" is already used by <other file>` | Two articles share a title (or `slug:`) — retitle/rename one |
| `slug "…" must be lowercase kebab-case…` | Your `slug:` frontmatter has spaces, capitals, or a stray hyphen — fix the value |
| `running rsync — is it installed…` / ssh errors | Test `ssh deploy@your-server-ip echo ok`; your key must be in the server's `authorized_keys` |
| Published but browser 404s | Usually DNS/browser cache — `curl -s -o /dev/null -w '%{remote_ip}\n' https://example.com/` should print your server's IP |
| exit code `2` | Not an error: content unchanged since last publish (`--force` to override) |

## Mental model in one paragraph

`mdpub` never talks to a database or an API. Your markdown files are the
source of truth; `blog/` is a build artifact you can regenerate; the
server is a dumb mirror of `blog/public/`. The only state is
`.mdpub-state.json` — a map of *file → (slug, content hash, URL)* used to
skip no-op publishes, detect renames, and power `status`. Delete it and
nothing breaks; the next publish simply re-records.

## 6. Moderating comments and contact messages

Readers can leave comments, reactions, and contact-form messages, all
served by `blog-api` (see [server-setup.md](server-setup.md) §8) — a
separate service from `mdpub`, sitting behind `/api/*`. New comments land
as *pending* until approved. Moderate them with `curl`, authenticating
with the same bearer token you put in `/etc/blog-api.env` on the server
(`BLOG_API_ADMIN_TOKEN`):

```bash
export TOKEN=<your BLOG_API_ADMIN_TOKEN>

# list comments awaiting moderation
curl -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/admin/comments/pending

# approve one (makes it publicly visible)
curl -X POST -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/admin/comments/42/approve

# delete one (spam, abuse, etc.)
curl -X DELETE -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/admin/comments/42

# view contact-form submissions
curl -H "Authorization: Bearer $TOKEN" \
  https://example.com/api/admin/contact
```

### Skipping moderation (auto-approve)

If the approval step is more friction than it's worth, set
`BLOG_API_AUTO_APPROVE=1` in `/etc/blog-api.env` and
`systemctl restart blog-api` — new comments then publish immediately
instead of landing as *pending*. The honeypot, rate limit, and length
checks still apply; only the human-in-the-loop is removed. Leave it unset
(the default) to keep moderation on. It's a runtime toggle: flip it back
to `0` and restart if spam ever shows up — no redeploy needed.

Reactions and contact-form submissions are always immediate; only
comments are gated by moderation.

This interactivity layer is entirely independent of `mdpub publish` —
mdpub still just mirrors static files with `rsync --delete` and never
touches `blog-api` or its SQLite database.
