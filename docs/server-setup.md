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
