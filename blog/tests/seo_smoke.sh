#!/usr/bin/env bash
# SEO smoke test for the Zola templates (F6: Open Graph / Twitter cards / canonical).
#
# Tera fails soft: a renamed or dropped variable renders as an empty string rather
# than erroring, which is exactly how `canonical_url` went silently missing in the
# first place. `zola build` succeeding therefore proves nothing about the head tags.
# These assertions pin the rendered output.
#
# Usage: blog/tests/seo_smoke.sh          (run from anywhere)
set -euo pipefail

SITE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

fails=0
check() { # check <name> <haystack-file> <needle>
  if grep -qF -- "$3" "$2"; then
    echo "  ok    $1"
  else
    echo "  FAIL  $1"
    echo "        expected to find: $3"
    fails=$((fails + 1))
  fi
}
check_absent() { # check_absent <name> <haystack-file> <needle>
  if grep -qF -- "$3" "$2"; then
    echo "  FAIL  $1"
    echo "        expected NOT to find: $3"
    fails=$((fails + 1))
  else
    echo "  ok    $1"
  fi
}

# Build an isolated copy so we can inject fixtures without touching the repo.
cp -R "$SITE_DIR" "$WORK/site"
rm -rf "$WORK/site/public"
cd "$WORK/site"

# Fixture: a cross-posted article. No real post sets canonical_url, so without
# this the override path — the actual F6 bug — would go untested.
CANON="https://example.com/canonical-target"
POST="content/blog/embedding-table/index.md"
python3 - "$POST" "$CANON" <<'PY'
import sys
path, canon = sys.argv[1], sys.argv[2]
s = open(path).read()
assert "[extra]" not in s, "fixture assumes no [extra] table yet"
head, sep, body = s.partition("+++\n\n")
open(path, "w").write(head + f'\n[extra]\ncanonical_url = "{canon}"\n' + sep + body)
PY

zola build >/dev/null 2>&1 || { echo "FAIL: zola build errored"; exit 1; }
echo "zola build: ok"

# Zola HTML-escapes '/' as &#x2F; in attributes; decode so assertions read plainly.
decode() { sed 's/&#x2F;/\//g; s/&#x27;/'"'"'/g; s/&amp;/\&/g' "$1" > "$2"; }

decode public/blog/embedding-table/index.html "$WORK/article.html"
decode public/404.html                        "$WORK/404.html"
decode public/contact/index.html              "$WORK/contact.html"
decode public/index.html                      "$WORK/home.html"

echo "article (cross-posted):"
# The F6 bug: canonical_url frontmatter must win over the local permalink.
check "canonical honors [extra].canonical_url" "$WORK/article.html" \
  "<link rel=\"canonical\" href=\"$CANON\">"
# og:url intentionally stays the local permalink so social engagement aggregates here.
check "og:url stays local permalink" "$WORK/article.html" \
  '<meta property="og:url" content="http://127.0.0.1:1111/blog/embedding-table/">'
check "og:type is article"        "$WORK/article.html" '<meta property="og:type" content="article">'
check "og:image is absolute"      "$WORK/article.html" \
  '<meta property="og:image" content="http://127.0.0.1:1111/blog/embedding-table/01_bpe_training.gif">'
check "twitter large card"        "$WORK/article.html" '<meta name="twitter:card" content="summary_large_image">'
check "article:published_time"    "$WORK/article.html" '<meta property="article:published_time"'
# No post sets `description`, so this must fall back to real body text rather than
# emitting config.description identically on every post.
check_absent "og:description is not the generic site blurb" "$WORK/article.html" \
  '<meta property="og:description" content="Notes on software, APIs, and AI systems">'

echo "404:"
# A canonical here would tell crawlers the error page *is* the homepage.
check        "noindex"          "$WORK/404.html" '<meta name="robots" content="noindex">'
check_absent "no canonical"     "$WORK/404.html" '<link rel="canonical"'

echo "contact (undated page):"
check "og:type is website, not article" "$WORK/contact.html" '<meta property="og:type" content="website">'
check "canonical present"               "$WORK/contact.html" \
  '<link rel="canonical" href="http://127.0.0.1:1111/contact/">'

echo "home:"
check "og:type is website" "$WORK/home.html" '<meta property="og:type" content="website">'
check "canonical is root"  "$WORK/home.html" '<link rel="canonical" href="http://127.0.0.1:1111/">'

echo
if [ "$fails" -ne 0 ]; then
  echo "seo_smoke: $fails check(s) FAILED"
  exit 1
fi
echo "seo_smoke: all checks passed"
