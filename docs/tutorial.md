# mdpub tutorial: from markdown file to live article

This is the full walkthrough of publishing with `mdpub`. It assumes the
server is already set up (if not, do [server-setup.md](server-setup.md)
first — it's a one-time job).

## 0. One-time setup on a new machine

```bash
brew install zola                      # static site generator (build happens locally)
cargo install --path mdpub             # from the repo root
mdpub init --server deploy@172.104.62.92 --base-url https://edwardhong.net
```

`mdpub init` writes `mdpub.toml` (git-ignored, since it names your
server). Everything below is run from anywhere inside the repo — `mdpub`
finds `mdpub.toml` by walking up parent directories, like git does.

## 1. Write an article

Create a markdown file anywhere in the repo, e.g. `Day2/why-caching.md`:

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
- **Date** — defaults to today.
- **Images** — `cache-flow.png` is resolved relative to the `.md` file,
  copied into the site next to the article, and deployed with it. Remote
  images (`https://…`) are left as-is. A typo'd image path aborts the
  publish before anything is built or deployed.

Want more control? Add YAML frontmatter at the very top:

```markdown
---
title: Why Your Cache Invalidation Story Is Probably Fine
tags: [caching, systems]
date: 2026-07-12
description: The two-hard-problems joke, taken seriously for once.
---
```

Unknown frontmatter keys are rejected (typo protection), tags can be a
list or `tags: caching, systems`, and dates accept `YYYY-MM-DD` or
RFC 3339.

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
  Live:   https://edwardhong.net/blog/why-your-cache-invalidation-story-is-probably-fine/
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
Day1/mcp-vs-rest-design.md  [published]  https://edwardhong.net/blog/mcp-is-not-…/
Day2/why-caching.md         [changed since publish]  https://edwardhong.net/blog/why-…/
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
| `slug "…" is already used by <other file>` | Two articles share a title — retitle one |
| `running rsync — is it installed…` / ssh errors | Test `ssh deploy@172.104.62.92 echo ok`; your key must be in the server's `authorized_keys` |
| Published but browser 404s | Usually DNS/browser cache — `curl -s -o /dev/null -w '%{remote_ip}\n' https://edwardhong.net/` should print `172.104.62.92` |
| exit code `2` | Not an error: content unchanged since last publish (`--force` to override) |

## Mental model in one paragraph

`mdpub` never talks to a database or an API. Your markdown files are the
source of truth; `blog/` is a build artifact you can regenerate; the
server is a dumb mirror of `blog/public/`. The only state is
`.mdpub-state.json` — a map of *file → (slug, content hash, URL)* used to
skip no-op publishes, detect renames, and power `status`. Delete it and
nothing breaks; the next publish simply re-records.
