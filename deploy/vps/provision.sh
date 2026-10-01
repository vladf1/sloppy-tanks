#!/usr/bin/env bash
# Idempotent setup of the self-hosted multiplayer server on Ubuntu. Run as root from a
# directory holding this script and the other files in deploy/vps/;
# `node scripts/deploy-vps.mjs --provision` uploads them and runs it.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive

# Caddy's former apt repository fails apt-get update (install-caddy.sh says why).
rm -f /etc/apt/sources.list.d/caddy-stable.list /usr/share/keyrings/caddy-stable-archive-keyring.gpg
apt-get update
apt-get install -y ca-certificates curl ufw

. ./install-caddy.sh
. ./install-docker.sh

install -m 644 sloppy-tanks.env /etc/sloppy-tanks.env
install -m 644 sloppy-tanks.service /etc/systemd/system/sloppy-tanks.service
install -m 644 sloppy-tanks-dev.env /etc/sloppy-tanks-dev.env
install -m 644 sloppy-tanks-dev.service /etc/systemd/system/sloppy-tanks-dev.service
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
# The auto-update timer stays as it is: `pnpm run server:auto-update on|off` owns it.
systemctl enable sloppy-tanks sloppy-tanks-dev caddy
systemctl reload-or-restart caddy
# A service starts once a deploy has pinned its image. Until then, a server started
# by the former binary unit keeps running and the first deploy replaces it.
if [ -f /var/lib/sloppy-tanks/production.image ]; then systemctl restart sloppy-tanks; fi
if [ -f /var/lib/sloppy-tanks/dev.image ]; then systemctl restart sloppy-tanks-dev; fi

# Caddy needs 80 for the Let's Encrypt HTTP challenge, 443 for wss and 8443 for the dev server.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 8443/tcp
ufw --force enable
