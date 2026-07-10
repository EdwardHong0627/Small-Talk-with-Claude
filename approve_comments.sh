#!/usr/bin/env bash
# Approve ALL pending comments via the blog-api admin endpoints.
#
# Usage:
#   TOKEN=<admin bearer token> ./approve_comments.sh
#
# Base URL comes from mdpub.toml's base_url (override with BLOG_URL=...).
set -euo pipefail

command -v jq >/dev/null || { echo "jq is required (brew install jq)" >&2; exit 1; }

repo_root="$(cd "$(dirname "$0")" && pwd)"
base_url="${BLOG_URL:-$(sed -n 's/^base_url *= *"\(.*\)"/\1/p' "$repo_root/mdpub.toml")}"
: "${base_url:?no base URL — set BLOG_URL or add base_url to mdpub.toml}"
: "${TOKEN:?set TOKEN to the blog-api admin bearer token}"

pending="$(curl -fsS -H "Authorization: Bearer $TOKEN" \
  "$base_url/api/admin/comments/pending")"

count="$(printf '%s' "$pending" | jq 'length')"
if [ "$count" -eq 0 ]; then
  echo "no pending comments"
  exit 0
fi

printf '%s' "$pending" | jq -r '.[] | "\(.id)\t\(.slug)\t\(.author)"' |
while IFS=$'\t' read -r id slug author; do
  curl -fsS -X POST -H "Authorization: Bearer $TOKEN" \
    "$base_url/api/admin/comments/$id/approve" >/dev/null
  echo "approved #$id on '$slug' by $author"
done

echo "approved $count comment(s)"
