# Phase 0 — Stop the data-loss window

## Context

The improvement-plan review found that `.mdpub-state.json` tracks 6 published articles but only
4 imported content dirs exist under `blog/content/blog/` — the Day5 (ClickHouse) and Day6
(embedding-table) imports were never committed on any branch. Since `mdpub publish` runs
`zola build` + `rsync --delete` (`mdpub/src/deploy.rs:23`), publishing anything from this checkout
would silently delete both live posts from edwardhong.net. Separately, the comment and contact
forms label the name field "(optional)" while the API rejects empty values with a 400
(`blog-api/src/routes/comments.rs:70`, `contact.rs:31`), and the UI swallows the API's error text.

Phase 0 = restore the missing imports, guard mdpub against this drift class permanently, and fix
the empty-author/name bug.

**Decisions from user:** user runs the final deploying publish themselves (I verify); contact-form
fix is in scope; surfacing API error text in `interact.js` is in scope.

## Branch

New branch `fix/phase0-drift-and-anon-comments` off `origin/main` (keeps the unmerged lightbox
branch out of the deploy). One PR to `main` at the end.

## Workstream 1 — mdpub drift guard (F1 prevention)

Key facts from exploration (`mdpub/src/lib.rs`):
- `publish` flow: import target article only (`lib.rs:121-131`) → `deploy::build` (`:133`) →
  dry-run gate returns before deploy/state (`:150-153`) → `deploy::deploy` (`:155`) → state save.
- Unchanged short-circuit exits 2 at `lib.rs:98-102`; date stability via `published_at` reuse
  (`lib.rs:77-81`); `status` label logic at `lib.rs:191-202` checks only the *source* file.
- Reuse: `ws.content_dir()` (`config.rs:69-71`), `st.articles: BTreeMap<String, Article>`
  (`state.rs:19-26`), the `content_dir.join(slug).exists()` pattern from `remove_page` (`lib.rs:255`).

Changes in `mdpub/src/lib.rs`:
1. **Publish guard**: before `deploy::deploy` (only when `!dry_run`), iterate `st.articles`; for
   every entry *other than the current key*, require `ws.content_dir().join(&article.slug)` to
   exist. If any are missing, `bail!` (exit 1) listing each missing key/slug with the remedy:
   `mdpub publish <key> --dry-run` to regenerate the import, or `mdpub unpublish <key>`.
   Dry-run publishes are exempt (they never deploy — and they're the remedy).
2. **Self-heal**: skip the exit-2 "unchanged" short-circuit (`lib.rs:98-102`) when the *current*
   article's own content dir is missing, so a plain republish regenerates it.
3. **Status**: in `fn status` (`lib.rs:191-202`), add a `missing import (run publish --dry-run)`
   label when the content dir for a tracked slug is absent (after the missing/unreadable-source
   checks, before hash comparison).

Tests (mirror `fixture()` + `MockRunner` unit style, `lib.rs:286-342`, and `Fixture`/stub-binary
CLI style, `tests/cli.rs:19-74`):
- `publish_refuses_when_another_import_is_missing` — two articles published, delete one's content
  dir, publish the other → `Err`, no `rsync` call recorded in `MockRunner`.
- `unchanged_publish_regenerates_missing_own_import` — publish, delete own content dir, republish
  without `--force` → not exit 2, dir recreated, deploy runs.
- `dry_run_allowed_when_imports_missing` — dry-run succeeds despite another missing import.
- `status_flags_missing_import` — status shows the new label.

## Workstream 2 — blog-api empty author/name (F2)

- `blog-api/src/routes/comments.rs:70`: `validate_len("author", ., 1, 80)` → min `0`.
- `blog-api/src/routes/contact.rs:31`: `validate_len("name", ., 1, 80)` → min `0`.
- No migration needed (`author`/`name` are `TEXT NOT NULL`; `''` is fine — `0001_init.sql:6,29`).
  Admin flow unaffected; `renderComment` already falls back to `'anonymous'` for `""`
  (`interact.js:128`). No existing test asserts rejection, so nothing to update.

Tests (mirror `blog-api/tests/comments.rs:57` `auto_approve_comment_visible_immediately` and
helpers in `tests/common/mod.rs`):
- `empty_author_comment_accepted` — POST with `"author": ""` → 200; approve; GET returns it.
- `empty_name_contact_accepted` — POST `/api/contact` with `"name": ""` → 200.

## Workstream 3 — surface API error text (`blog/static/js/interact.js`)

In the comment (`:199-201`) and contact (`:250-252`) submit handlers: on `!res.ok`, attempt
`res.json()` and, if it has a string `error` field, show that in the status element; otherwise keep
the current generic message. Preserve the XSS rule — assign via `textContent` only.

## Workstream 4 — restore Day5/Day6 (F1 restore)

Run with the updated binary (`cargo install --path mdpub` after Workstream 1):
1. `mdpub publish Day5/ClickHouse_MergeTree.md --dry-run` — regenerates
   `blog/content/blog/clickhouse-mergetree-parts-merges-and-the-meaning-of-primary-key/`
   (no deploy, no state write; date preserved from state per `lib.rs:77-81`).
2. `mdpub publish Day6/from-text-to-vectors.md --dry-run` — regenerates
   `blog/content/blog/embedding-table/`.
3. Verify: both dirs exist with `index.md` + images; frontmatter dates match `.mdpub-state.json`
   `published_at`; `zola build` output shows all 6 posts; `mdpub status` shows no missing imports.
4. Commit the two content dirs.
5. **User runs the deploy**: `mdpub publish Day6/from-text-to-vectors.md --force`
   (all imports now present, so the guard passes and the deployed site contains all 6 posts).
6. I verify post-deploy: `curl -sI` all 6 live URLs → 200, including the two restored posts.

## Verification

- `cargo test` in `mdpub/` and `blog-api/` — all pass (76 existing + new).
- Keep new code rustfmt-clean, but do NOT run crate-wide `cargo fmt` (pre-existing drift is a
  Phase 1 item; don't mix it into this diff).
- Manual end-to-end: step 6 above (live URLs), plus `mdpub status` listing all 6 as `published`.
- PR `fix/phase0-drift-and-anon-comments` → `main`.

## Out of scope (later phases)

CI, handler `unwrap()` hardening, backups, SEO meta, security headers, rate-limiter eviction,
GIF/font perf, notifications, repo hygiene.
