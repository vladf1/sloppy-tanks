#!/usr/bin/env bash
# Idempotent setup of the dev multiplayer server beside production on the same VPS. It
# installs Docker, the image updater, the dev unit and environment, Caddy's config and
# the 8443 firewall rule; the production unit and process are left as they are. Run as
# root from a directory holding this script and the files deploy-vps.mjs --dev
# --provision uploads with it.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive
command -v caddy >/dev/null || { echo "Caddy is missing; run the full provision.sh first" >&2; exit 1; }
# Caddy's former apt repository fails apt-get update (install-caddy.sh says why); the
# installed Caddy keeps running.
rm -f /etc/apt/sources.list.d/caddy-stable.list /usr/share/keyrings/caddy-stable-archive-keyring.gpg
apt-get update
. ./install-docker.sh
install -m 644 sloppy-tanks-dev.env /etc/sloppy-tanks-dev.env
install -m 644 sloppy-tanks-dev.service /etc/systemd/system/sloppy-tanks-dev.service
# Validate before replacing the live config, so a mistake cannot take production down.
caddy validate --config Caddyfile --adapter caddyfile
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
systemctl enable sloppy-tanks-dev
systemctl reload caddy
# The service starts once a deploy has pinned its image. Until then, a server started
# by the former binary unit keeps running and the first deploy replaces it.
if [ -f /var/lib/sloppy-tanks/dev.image ]; then systemctl restart sloppy-tanks-dev; fi
ufw allow 8443/tcp
