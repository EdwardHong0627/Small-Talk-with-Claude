#!/usr/bin/env bash
#
# Deploy blog-api to the Linode.
#
# One-time setup on the dev Mac (Apple Silicon macOS -> x86_64 Linux server,
# so a plain `cargo build --release` produces the wrong OS/arch binary; we
# cross-compile a static musl binary instead):
#
#   brew install zig
#   cargo install cargo-zigbuild
#   rustup target add x86_64-unknown-linux-musl
#
# rusqlite's `bundled` feature statically links SQLite into the binary, so
# there's nothing to install server-side and no libsqlite version drift.
#
# Do NOT build on the server itself: it's a 1 GB Nanode and compiling
# axum/tokio there will OOM.
#
# Usage:
#   ./deploy.sh              # cross-compile, ship binary + migrations, restart service
#   ./deploy.sh deploy       # same as above
#   ./deploy.sh pull-backup  # fetch the latest nightly SQLite backup to this machine
#
# Config (env vars, all optional):
#   BLOG_API_SERVER        deploy target, e.g. deploy@203.0.113.7 (default below)
#   BLOG_API_INSTALL_PATH  remote path the systemd unit's ExecStart points at
#   BLOG_API_MIGRATIONS_DIR remote dir for migrations (under StateDirectory)
#   BLOG_API_BACKUP_DIR     remote dir nightly backups land in
#
# No secrets live in this script or in git. BLOG_API_ADMIN_TOKEN lives only
# in /etc/blog-api.env on the server (see docs/server-setup.md).

set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")"

BLOG_API_SERVER="${BLOG_API_SERVER:-deploy@203.0.113.7}"
BLOG_API_INSTALL_PATH="${BLOG_API_INSTALL_PATH:-/usr/local/bin/blog-api}"
BLOG_API_MIGRATIONS_DIR="${BLOG_API_MIGRATIONS_DIR:-/var/lib/blog-api/migrations}"
BLOG_API_BACKUP_DIR="${BLOG_API_BACKUP_DIR:-/var/backups/blog-api}"

TARGET="x86_64-unknown-linux-musl"
BIN_PATH="target/${TARGET}/release/blog-api"

cmd="${1:-deploy}"

case "$cmd" in
  deploy)
    echo "==> cross-compiling blog-api for ${TARGET}"
    cargo zigbuild --release --target "${TARGET}"

    echo "==> verifying static linkage"
    file "${BIN_PATH}"
    if ! file "${BIN_PATH}" | grep -q 'ELF 64-bit.*x86-64'; then
      echo "error: ${BIN_PATH} is not a statically-linked x86-64 ELF binary" >&2
      exit 1
    fi
    if file "${BIN_PATH}" | grep -qi 'dynamically linked'; then
      echo "error: ${BIN_PATH} is dynamically linked, expected static" >&2
      exit 1
    fi

    echo "==> shipping binary to ${BLOG_API_SERVER}:${BLOG_API_INSTALL_PATH}"
    # scp to a temp path, then move into place with sudo on the remote side —
    # this is where systemd's ExecStart (blog-api.service) expects to find it.
    scp "${BIN_PATH}" "${BLOG_API_SERVER}:/tmp/blog-api.new"
    ssh "${BLOG_API_SERVER}" "sudo mv /tmp/blog-api.new '${BLOG_API_INSTALL_PATH}' && sudo chmod 755 '${BLOG_API_INSTALL_PATH}'"

    echo "==> shipping migrations to ${BLOG_API_SERVER}:${BLOG_API_MIGRATIONS_DIR}"
    ssh "${BLOG_API_SERVER}" "sudo mkdir -p '${BLOG_API_MIGRATIONS_DIR}'"
    scp -r migrations/. "${BLOG_API_SERVER}:/tmp/blog-api-migrations"
    ssh "${BLOG_API_SERVER}" "sudo cp -r /tmp/blog-api-migrations/. '${BLOG_API_MIGRATIONS_DIR}/' && rm -rf /tmp/blog-api-migrations"

    echo "==> running migrations"
    ssh "${BLOG_API_SERVER}" "sudo '${BLOG_API_INSTALL_PATH}' --migrate-only"

    echo "==> restarting blog-api service"
    ssh "${BLOG_API_SERVER}" "sudo systemctl restart blog-api"

    echo "==> done"
    ;;

  pull-backup)
    # Backups are written by deploy/backup-blog-api.sh (nightly cron on the
    # server) as blog-api-YYYY-MM-DD.sqlite3.gz. Pull the newest one.
    remote_file="$(ssh "${BLOG_API_SERVER}" \
      "ls -1t '${BLOG_API_BACKUP_DIR}'/blog-api-*.sqlite3.gz 2>/dev/null | head -n 1")"
    if [ -z "${remote_file}" ]; then
      echo "error: no backups found in ${BLOG_API_SERVER}:${BLOG_API_BACKUP_DIR}" >&2
      exit 1
    fi
    local_file="$(basename "${remote_file}")"

    echo "==> pulling ${BLOG_API_SERVER}:${remote_file} -> ./${local_file}"
    scp "${BLOG_API_SERVER}:${remote_file}" "./${local_file}"
    echo "==> done"
    ;;

  *)
    echo "usage: $0 [deploy|pull-backup]" >&2
    exit 1
    ;;
esac
