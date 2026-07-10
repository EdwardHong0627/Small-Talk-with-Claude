# Plan: `mdpub` — Markdown publishing CLI for a self-hosted blog on Linode

## Context

The original goal was a markdown→Medium publishing CLI. Research showed Medium **no longer issues API integration tokens** (existing tokens work; new users are locked out), leaving only fragile or manual workarounds (clipboard paste, browser automation). The user rejected those as too inconvenient and pivoted: **self-host the blog on a Linode server** and have the CLI publish markdown there instead — full automation, no third-party API risk.

Decisions made with the user:
- **Blog stack**: Zola (Rust single-binary static site generator) served by Caddy on Linode
- **Language**: Rust CLI (`mdpub`)
- **Deploy**: `rsync` over SSH of the built site to the Linode docroot
- **Linode**: nothing provisioned yet — plan includes one-time server setup
- **v1 features**: frontmatter metadata, dry-run/preview, publish tracking (local image handling deferred)

Workflow after this plan: write an article anywhere in the repo (e.g. `Day1/mcp-vs-rest-design.md`), run `mdpub publish Day1/mcp-vs-rest-design.md`, and it's live at `https://<domain>/blog/<slug>/`.

## Architecture

```
repo (this workspace)
├── Day1/…                    # source articles, written anywhere, YAML frontmatter optional
├── blog/                     # Zola site (zola init): config.toml, templates/, content/blog/
├── mdpub/                    # Rust CLI cargo project
├── mdpub.toml                # CLI config: server host/user, docroot, site dir, base URL
└── .mdpub-state.json         # publish tracking: source path → slug, hash, URL, timestamp

mdpub publish Day1/foo.md
  1. parse YAML frontmatter (title, tags, date, draft) — fallbacks: first h1 → title, file mtime → date
  2. translate → Zola TOML frontmatter, write to blog/content/blog/<slug>.md
  3. zola build (subprocess)
  4. rsync -az --delete blog/public/ deploy@<linode>:/var/www/blog/
  5. record hash+URL in .mdpub-state.json, print live URL
```

## Part A — One-time Linode + web server setup (manual/scripted, documented in `docs/server-setup.md`)

1. **Provision**: Linode Nanode 1GB (~$5/mo), Ubuntu 24.04 LTS, SSH public key added at creation (Cloud Manager or `linode-cli linodes create`).
2. **Harden**: create `deploy` user with the SSH key; `ufw allow OpenSSH, 80, 443; ufw enable`; disable SSH password auth.
3. **Caddy** (chosen over nginx: automatic Let's Encrypt TLS, 2-line config): install from official apt repo. `Caddyfile`:
   ```
   <domain> {
       root * /var/www/blog
       file_server
   }
   ```
   `mkdir -p /var/www/blog && chown deploy /var/www/blog`.
4. **DNS**: A record for the domain → Linode IP. **If no domain yet**: serve `http://<ip>` (Caddyfile site address `:80`) and switch to the domain block later — CLI is agnostic, it only rsyncs.
5. Verify: `echo hi > /var/www/blog/index.html` → visible in browser.

## Part B — Zola site scaffold

- `zola init blog` (install locally: `brew install zola`; on CI/server not needed — build happens locally).
- Minimal own templates (base/index/page) or a lightweight theme; `config.toml`: `base_url`, `generate_feeds = true`, `taxonomies = [{name = "tags"}]`.
- `content/blog/_index.md` for the post list section.
- Articles get TOML frontmatter: `title`, `date`, `draft`, `[taxonomies] tags`, `[extra] canonical_url`.

## Part C — Rust CLI `mdpub` (cargo project in `mdpub/`)

### Crates
| Crate | Why |
|---|---|
| `clap` 4 (derive) | subcommands |
| `serde`/`serde_json` | state file |
| `serde_yaml_ng` | parse source YAML frontmatter (maintained serde_yaml fork) |
| `toml` | emit Zola frontmatter + read `mdpub.toml` config |
| `blake3` | content hash for change detection |
| `chrono` | dates |
| `anyhow`, `tempfile`, `open` | errors, temp files, browser |
| dev: `assert_cmd`, `predicates`, `insta` | CLI + snapshot tests |

`zola` and `rsync` run as subprocesses via `std::process::Command`, **behind a `Runner` trait** so tests mock them (same pattern for filesystem-visible effects; no network in tests).

### CLI surface
```
mdpub init                          # write mdpub.toml (prompts for server, docroot, base_url)
mdpub publish <file.md> [--dry-run] [--force] [--draft]
mdpub preview                       # zola serve, open http://127.0.0.1:1111
mdpub status                        # tracked articles: published / changed / untracked
mdpub unpublish <file.md>           # remove from content dir, rebuild, redeploy
```

- `--dry-run`: do steps 1–3 (import + build) but skip rsync; print what would deploy.
- Unchanged already-published file → warn + exit 2 unless `--force`.
- Slug: kebab-case of title (frontmatter or first h1), collision-checked against state file.

### Modules
```
mdpub/src/
  main.rs          # wire real Runner, parse CLI
  cli.rs           # clap derive
  config.rs        # mdpub.toml load/validate
  frontmatter.rs   # YAML split/parse → Meta {title, tags, date, draft, canonical_url}; h1/mtime fallbacks
  zola.rs          # Meta+body → TOML-frontmatter page, slug logic, content-dir write
  runner.rs        # trait Runner {run(cmd)->Result}; RealRunner; MockRunner (tests)
  deploy.rs        # zola build + rsync invocations via Runner
  state.rs         # .mdpub-state.json: load/save atomic, hash, status
```

### Tests (required for all generated code)
- `frontmatter.rs`: full/partial/missing YAML, CRLF, `---` in body, h1-fallback (exactly the `Day1` article shape — it has no frontmatter, title from line 1 h1).
- `zola.rs`: YAML→TOML translation snapshot tests, slug generation/collision, draft flag.
- `state.rs`: roundtrip, change detection, atomic write.
- `deploy.rs` with MockRunner: publish invokes `zola build` then `rsync` with expected args; `--dry-run` invokes build only.
- `tests/cli.rs` (assert_cmd): end-to-end against a temp Zola site fixture; exit-code-2 on unchanged republish.

## Implementation order
1. Zola site scaffold (`blog/`), verify `zola build` + `zola serve` locally.
2. `mdpub` cargo project: config + frontmatter + zola modules with tests.
3. state + deploy + CLI wiring with tests.
4. Server setup (Part A) — do this in parallel/anytime; document real IP/host in `mdpub.toml`.
5. End-to-end verification (below).

## Verification
```bash
cd mdpub && cargo test                          # all unit + integration tests
cargo run -- publish ../Day1/mcp-vs-rest-design.md --dry-run
#   expect: title "MCP Is Not a New Paradigm…", slug generated, zola build OK, no rsync
cargo run -- preview                            # article renders correctly at localhost:1111
#   (check: headings, code blocks, the section-2 table — Zola renders GFM tables natively,
#    a bonus over Medium which would have dropped them)
cargo run -- publish ../Day1/mcp-vs-rest-design.md
#   expect: rsync runs, prints https://<domain>/blog/<slug>/ — open it in a browser
cargo run -- status                             # shows published
cargo run -- publish ../Day1/mcp-vs-rest-design.md   # exit 2 (unchanged) without --force
```

## Out of scope for v1
- Local image upload/rewriting (Zola colocated assets make this easy to add later)
- Medium cross-posting (revisit only if a token ever becomes available)
- CI-based deploys, comments, analytics
