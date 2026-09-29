#!/usr/bin/env bash
# Idempotent setup of the dev multiplayer server beside production on the same VPS. It
# installs only the dev unit and environment, Caddy's config and the 8443 firewall rule;
# the production unit and process are left as they are. Run as root from a directory
# holding this script, Caddyfile, sloppy-tanks-dev.service and sloppy-tanks-dev.env;
# `node scripts/deploy-vps.mjs --dev --provision` uploads them and runs it.
set -euo pipefail
cd "$(dirname "$0")"
command -v caddy >/dev/null || { echo "Caddy is missing; run the full provision.sh first" >&2; exit 1; }
id sloppy >/dev/null 2>&1 || { echo "The sloppy user is missing; run the full provision.sh first" >&2; exit 1; }
install -d -m 755 /opt/sloppy-tanks-dev
install -m 644 sloppy-tanks-dev.env /etc/sloppy-tanks-dev.env
install -m 644 sloppy-tanks-dev.service /etc/systemd/system/sloppy-tanks-dev.service
# Validate before replacing the live config, so a mistake cannot take production down.
caddy validate --config Caddyfile --adapter caddyfile
install -m 644 Caddyfile /etc/caddy/Caddyfile
systemctl daemon-reload
systemctl enable sloppy-tanks-dev
systemctl reload caddy
# The service starts once a deploy has uploaded the server binary.
if [ -f /opt/sloppy-tanks-dev/sloppy-server ]; then systemctl restart sloppy-tanks-dev; fi
ufw allow 8443/tcp
