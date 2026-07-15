#!/usr/bin/env bash
#
# Nightly SQLite backup for blog-api. Runs ON THE SERVER via cron (not the
# dev Mac). Takes an online, consistent snapshot of the live DB with
# `sqlite3 .backup` (safe to run while blog-api is up — unlike `cp`, which
# can copy a half-written page mid-write), gzips it, and drops it in
# /var/backups/blog-api/ with a 7-day rotation.
#
# One-time setup (see docs/server-setup.md for the full copy-paste block):
#
#   sudo cp backup-blog-api.sh /usr/local/bin/backup-blog-api.sh
#   sudo chmod +x /usr/local/bin/backup-blog-api.sh
#   sudo crontab -e
#
# Cron entry (3:17am daily — off the hour, avoids the top-of-hour pile-up):
#
#   17 3 * * * /usr/local/bin/backup-blog-api.sh
#
# Quiet on success, loud on stderr + non-zero exit on failure (cron mails
# stderr output to root by default, or wire this up to your MTA of choice).

set -euo pipefail

# BLOG_API_DB_PATH comes from /etc/blog-api.env (same file the blog-api
# systemd unit reads). Fall back to the path the unit hardcodes
# (blog-api.service: Environment=BLOG_API_DB_PATH=/var/lib/blog-api/blog.db),
# which also matches config.rs's `${STATE_DIRECTORY}/blog.db` convention
# for StateDirectory=blog-api.
if [ -f /etc/blog-api.env ]; then
  # shellcheck disable=SC1091
  source /etc/blog-api.env
fi
DB="${BLOG_API_DB_PATH:-/var/lib/blog-api/blog.db}"

BACKUP_DIR="/var/backups/blog-api"
KEEP=7

if [ ! -f "$DB" ]; then
  echo "backup-blog-api: DB not found at ${DB}" >&2
  exit 1
fi

mkdir -p "$BACKUP_DIR"

STAMP="$(date +%F)"
OUT="${BACKUP_DIR}/blog-api-${STAMP}.sqlite3"

# Online backup via the SQLite backup API — safe to run against a live DB,
# unlike `cp`/`rsync`, which can grab a torn/inconsistent snapshot mid-write.
sqlite3 "$DB" ".backup '${OUT}'"
gzip -f "$OUT"

# Rotation: keep only the newest $KEEP backups, delete the rest. `ls -1t`
# (newest first) handles the empty/short-directory case fine — `tail` on
# fewer than $KEEP+1 lines just prints nothing, so `rm -f` gets zero args
# only when no files matched, which `rm -f` treats as a no-op.
shopt -s nullglob
files=("${BACKUP_DIR}"/blog-api-*.sqlite3.gz)
shopt -u nullglob
count="${#files[@]}"
if [ "$count" -gt "$KEEP" ]; then
  # shellcheck disable=SC2012
  ls -1t "${BACKUP_DIR}"/blog-api-*.sqlite3.gz | tail -n "+$((KEEP + 1))" | \
    while IFS= read -r old; do
      rm -f -- "$old"
    done
fi
