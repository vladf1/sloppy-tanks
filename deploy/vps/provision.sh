#!/usr/bin/env bash
# Idempotent setup of the self-hosted multiplayer server on Ubuntu. Run as root from a
# directory holding this script, Caddyfile, sloppy-tanks.service and sloppy-tanks.env;
# `node scripts/deploy-vps.mjs --provision` uploads them and runs it.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive
NODE_MAJOR=24

apt-get update
apt-get install -y ca-certificates curl gnupg debian-keyring debian-archive-keyring ufw

# Ubuntu's own nodejs and caddy packages trail the versions this repo targets.
if [ ! -f /etc/apt/sources.list.d/nodesource.list ]; then
  curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key |
    gpg --dearmor --yes -o /usr/share/keyrings/nodesource.gpg
  echo "deb [signed-by=/usr/share/keyrings/nodesource.gpg] https://deb.nodesource.com/node_${NODE_MAJOR}.x nodistro main" \
    >/etc/apt/sources.list.d/nodesource.list
fi
if [ ! -f /etc/apt/sources.list.d/caddy-stable.list ]; then
  curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key |
    gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt \
    >/etc/apt/sources.list.d/caddy-stable.list
fi
apt-get update
apt-get install -y nodejs caddy
node --version | grep -q "^v${NODE_MAJOR}\." || {
  echo "Expected Node ${NODE_MAJOR}, found $(node --version)" >&2
  exit 1
}

id sloppy >/dev/null 2>&1 || useradd --system --home-dir /opt/sloppy-tanks --shell /usr/sbin/nologin sloppy
install -d -m 755 /opt/sloppy-tanks
install -m 644 sloppy-tanks.env /etc/sloppy-tanks.env
install -m 644 sloppy-tanks.service /etc/systemd/system/sloppy-tanks.service
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
systemctl enable sloppy-tanks caddy
systemctl reload-or-restart caddy
# The service starts once a deploy has uploaded server.mjs.
if [ -f /opt/sloppy-tanks/server.mjs ]; then systemctl restart sloppy-tanks; fi

# Caddy needs 80 for the Let's Encrypt HTTP challenge and 443 for wss.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable
