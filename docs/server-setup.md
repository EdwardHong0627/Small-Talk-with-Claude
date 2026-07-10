# One-time Linode server setup for the blog

Target: a Linode Nanode serving the Zola-built static site from
`/var/www/blog` via Caddy (automatic HTTPS). After this setup,
`mdpub publish <file.md>` is the only thing you ever run.

## 1. Provision the Linode

In [Linode Cloud Manager](https://cloud.linode.com) (or `linode-cli`):

- **Image**: Ubuntu 24.04 LTS
- **Plan**: Nanode 1 GB (~$5/mo — plenty for a static site)
- **Region**: nearest to your readers
- **SSH key**: add your public key (`cat ~/.ssh/id_ed25519.pub`) at creation
- Note the assigned IPv4 address — referred to as `<IP>` below.

CLI equivalent:

```bash
linode-cli linodes create \
  --type g6-nanode-1 --region ap-northeast --image linode/ubuntu24.04 \
  --label blog --root_pass '<strong-password>' \
  --authorized_keys "$(cat ~/.ssh/id_ed25519.pub)"
```

## 2. Create the deploy user and harden SSH

```bash
ssh root@<IP>

adduser --disabled-password --gecos "" deploy
mkdir -p /home/deploy/.ssh
cp /root/.ssh/authorized_keys /home/deploy/.ssh/
chown -R deploy:deploy /home/deploy/.ssh
chmod 700 /home/deploy/.ssh && chmod 600 /home/deploy/.ssh/authorized_keys

# firewall
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# key-only SSH
sed -i 's/^#\?PasswordAuthentication.*/PasswordAuthentication no/' /etc/ssh/sshd_config
systemctl restart ssh
```

Verify from your Mac before closing the root session:

```bash
ssh deploy@<IP> echo ok
```

## 3. Install Caddy and create the docroot

Still as root on the Linode:

```bash
apt update
apt install -y debian-keyring debian-archive-keyring apt-transport-https curl
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' \
  | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' \
  | tee /etc/apt/sources.list.d/caddy-stable.list
apt update && apt install -y caddy

mkdir -p /var/www/blog
chown deploy:deploy /var/www/blog
```

## 4. Configure Caddy

**With a domain** (recommended — Caddy gets a Let's Encrypt certificate
automatically). `/etc/caddy/Caddyfile`:

```
blog.example.com {
    root * /var/www/blog
    file_server
    encode gzip
}
```

**Without a domain yet** (plain HTTP on the IP):

```
:80 {
    root * /var/www/blog
    file_server
    encode gzip
}
```

Then:

```bash
systemctl reload caddy
```

## 5. DNS (if using a domain)

At your DNS provider, add an **A record**: `blog.example.com → <IP>`.
Caddy provisions TLS automatically on first request once DNS resolves.

## 6. Smoke test

```bash
ssh deploy@<IP> 'echo hello > /var/www/blog/index.html'
curl -s http://<IP>/          # or https://blog.example.com/
```

## 7. Point mdpub at the server

From the repo root:

```bash
mdpub init --server deploy@<IP> --base-url https://blog.example.com
# or, without a domain yet:
mdpub init --server deploy@<IP> --base-url http://<IP>
```

Then publish:

```bash
mdpub publish Day1/mcp-vs-rest-design.md
```

## Notes

- `mdpub` mirrors the whole built site with `rsync --delete`; nothing on
  the server ever needs manual editing.
- When you later add a domain, change `base_url` in `mdpub.toml`, swap
  the Caddyfile to the domain block, `systemctl reload caddy`, and
  `mdpub publish --force` any article to rebuild with the new URLs.
- Keep the system patched: `apt install unattended-upgrades` is a good
  idea on a box you rarely log into.

## 8. blog-api (comments / reactions / contact)

`blog-api` is a small axum service that adds comments, reactions, and a
contact form on top of the static site. It's a separate concern from
`mdpub`: Caddy proxies `/api/*` to it, everything else still goes to
`/var/www/blog`. Its SQLite database lives at `/var/lib/blog-api/blog.db`
— deliberately **outside** `/var/www/blog`, so `mdpub`'s `rsync --delete`
never touches it.

```
reader browser -> Caddy (443, auto-HTTPS)
   ├─ static files -> /var/www/blog        (mdpub's rsync target, unchanged)
   └─ reverse_proxy /api/* -> 127.0.0.1:8787  (blog-api, systemd-managed)
                                   -> SQLite at /var/lib/blog-api/blog.db
```

### Install the systemd unit

Copy the unit file to the server and create its env file (as root or via
`sudo`):

```bash
scp blog-api/deploy/blog-api.service deploy@<IP>:/tmp/
ssh deploy@<IP>
sudo mv /tmp/blog-api.service /etc/systemd/system/blog-api.service

sudo tee /etc/blog-api.env >/dev/null <<'EOF'
BLOG_API_ADMIN_TOKEN=<paste a long random value here>
EOF
sudo chmod 600 /etc/blog-api.env
```

`/etc/blog-api.env` is created manually and is **never committed** to
git. `DynamicUser=yes` + `StateDirectory=blog-api` in the unit mean
systemd auto-provisions both the service user and `/var/lib/blog-api` on
first start — no manual `useradd`/`mkdir` needed.

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now blog-api
```

### Caddy

Paste the `handle /api/*` block from `blog-api/deploy/caddy-snippet.conf`
into the existing site block in `/etc/caddy/Caddyfile`, **above**
`file_server` (Caddyfile matchers are order-sensitive — a `handle` placed
after `file_server` never fires). Then:

```bash
caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

### Firewall

No `ufw` change needed — blog-api binds `127.0.0.1:8787` only and is
reachable exclusively through Caddy.

### Nightly backup

A cron entry backs up the SQLite DB every night, rotating over the 7
weekdays:

```bash
sudo mkdir -p /var/lib/blog-api/backup
# crontab -e (as the user that can read /var/lib/blog-api, or root)
0 3 * * * sqlite3 /var/lib/blog-api/blog.db ".backup /var/lib/blog-api/backup/blog-$(date +\%u).db"
```

Pull the latest backup to your Mac with `blog-api/deploy.sh pull-backup`.
Litestream (continuous streaming replication of the SQLite WAL) is the
natural upgrade path if nightly snapshots aren't enough.

### First deploy of the binary

From the dev Mac:

```bash
blog-api/deploy.sh
```

This cross-compiles a static `x86_64-unknown-linux-musl` binary (the
Mac is Apple Silicon, the server is x86_64 Linux, so a plain `cargo
build --release` produces the wrong binary), ships it and the
`migrations/` directory to the server, runs migrations, and restarts the
service. See the comments at the top of `blog-api/deploy.sh` for
one-time setup (`zig`, `cargo-zigbuild`, the musl target).
