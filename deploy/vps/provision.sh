#!/usr/bin/env bash
# Idempotent setup of the self-hosted multiplayer server on Ubuntu. Run as root from a
# directory holding this script, Caddyfile and the sloppy-tanks(-dev) service and env files;
# `node scripts/deploy-vps.mjs --provision` uploads them and runs it.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive

apt-get update
apt-get install -y ca-certificates curl gnupg debian-keyring debian-archive-keyring ufw

# Ubuntu's own caddy package trails the version this repo targets. The game server
# is a static binary uploaded by deploy-vps.mjs and needs no runtime.
if [ ! -f /etc/apt/sources.list.d/caddy-stable.list ]; then
  curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/gpg.key |
    gpg --dearmor --yes -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
  curl -fsSL https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt \
    >/etc/apt/sources.list.d/caddy-stable.list
fi
apt-get update
apt-get install -y caddy

id sloppy >/dev/null 2>&1 || useradd --system --home-dir /opt/sloppy-tanks --shell /usr/sbin/nologin sloppy
install -d -m 755 /opt/sloppy-tanks /opt/sloppy-tanks-dev
install -m 644 sloppy-tanks.env /etc/sloppy-tanks.env
install -m 644 sloppy-tanks.service /etc/systemd/system/sloppy-tanks.service
install -m 644 sloppy-tanks-dev.env /etc/sloppy-tanks-dev.env
install -m 644 sloppy-tanks-dev.service /etc/systemd/system/sloppy-tanks-dev.service
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
systemctl enable sloppy-tanks sloppy-tanks-dev caddy
systemctl reload-or-restart caddy
# The service starts once a deploy has uploaded the server binary.
if [ -f /opt/sloppy-tanks/sloppy-server ]; then systemctl restart sloppy-tanks; fi
if [ -f /opt/sloppy-tanks-dev/sloppy-server ]; then systemctl restart sloppy-tanks-dev; fi

# Caddy needs 80 for the Let's Encrypt HTTP challenge, 443 for wss and 8443 for the dev server.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 8443/tcp
ufw --force enable
