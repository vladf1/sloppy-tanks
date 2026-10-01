#!/usr/bin/env bash
# Idempotent setup of the self-hosted multiplayer server on Ubuntu. Run as root from a
# directory holding this script and the other files in deploy/vps/;
# `node scripts/deploy-vps.mjs --provision` uploads them and runs it. The optional
# argument, podman or docker, switches both servers to that container runtime
# (install-runtime.sh); without it they keep the one they use.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive

# Caddy's former apt repository fails apt-get update (install-caddy.sh says why).
rm -f /etc/apt/sources.list.d/caddy-stable.list /usr/share/keyrings/caddy-stable-archive-keyring.gpg
apt-get update
apt-get install -y ca-certificates curl ufw

. ./install-caddy.sh
. ./install-runtime.sh

install -m 644 sloppy-tanks.env /etc/sloppy-tanks.env
install -m 644 sloppy-tanks-dev.env /etc/sloppy-tanks-dev.env
install_service sloppy-tanks
install_service sloppy-tanks-dev
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
# The auto-update timer stays as it is: `pnpm run server:auto-update on|off` owns it.
systemctl enable caddy
systemctl reload-or-restart caddy
start_service production sloppy-tanks
start_service dev sloppy-tanks-dev
stop_unused_docker

# Caddy needs 80 for the Let's Encrypt HTTP challenge, 443 for wss and 8443 for the dev server.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw allow 8443/tcp
ufw --force enable
