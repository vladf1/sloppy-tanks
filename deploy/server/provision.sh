#!/usr/bin/env bash
# Idempotent setup of one game server machine on Ubuntu: Podman, the server's and Caddy's
# Quadlet units, the image updater, key-only SSH and the firewall. Run as root from a
# directory holding this script and the other files in deploy/server/, with the machine's
# names written into the Caddyfile: bootstrap.sh fetches them at first boot, or
# `node scripts/server.mjs provision` uploads them with the machine's server.env. It
# starts no server image: the first deploy pins one.
set -euo pipefail
cd "$(dirname "$0")"
export DEBIAN_FRONTEND=noninteractive

# The recommended packages (Buildah, CRIU, docker-compose, GnuPG, rootless networking)
# are not needed to run prebuilt images as root. python3 parses /stats for auto-update.
apt-get update
apt-get install -y --no-install-recommends podman python3 curl ufw

# Keys only: the provider's root password stays usable from its web console. Earlier
# files win in sshd_config.d, so this one overrides cloud-init's.
printf '%s\n' 'PasswordAuthentication no' 'KbdInteractiveAuthentication no' \
  'PermitRootLogin prohibit-password' >/etc/ssh/sshd_config.d/10-sloppy-tanks.conf
sshd -t
# Ubuntu starts sshd per connection through ssh.socket; at first boot none runs yet, and
# one that starts later reads the new settings.
systemctl try-reload-or-restart ssh

install -d -m 755 /etc/sloppy-tanks/caddy /var/lib/sloppy-tanks
install -d -m 700 /var/lib/caddy/data /var/lib/caddy/config
# A restart ends live rooms, so the server restarts below only if its unit or settings
# changed. Without an uploaded server.env the one the user data wrote stays.
server_unit_changed=false
cmp -s sloppy-tanks.container /etc/containers/systemd/sloppy-tanks.container || server_unit_changed=true
if [[ -f server.env ]]; then
  cmp -s server.env /etc/sloppy-tanks/server.env || server_unit_changed=true
  install -m 644 server.env /etc/sloppy-tanks/server.env
fi
touch /etc/sloppy-tanks/server.env
# Check the Caddyfile with the Caddy it is for before it replaces a working one.
caddy_image=$(sed -n 's/^Image=//p' caddy.container)
podman run --rm --network none --volume "$PWD:/etc/caddy:ro" "$caddy_image" \
  caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
install -m 644 Caddyfile /etc/sloppy-tanks/caddy/Caddyfile
install -m 755 sloppy-tanks-update /usr/local/bin/sloppy-tanks-update
install -m 644 sloppy-tanks-update.service sloppy-tanks-update.timer /etc/systemd/system/
install -m 644 sloppy-tanks.container caddy.container /etc/containers/systemd/
# Dev's machines check :dev every 15 seconds, so a publish from a developer's checkout is
# live within moments; production keeps the timer's two minutes. The empty assignment
# clears the timer's interval before setting dev's, and AccuracySec lets it fire on time
# rather than anywhere within systemd's default minute.
timer_override=/etc/systemd/system/sloppy-tanks-update.timer.d/dev.conf
if grep -qx 'SERVER_ENVIRONMENT=dev' /etc/sloppy-tanks/server.env; then
  install -d /etc/systemd/system/sloppy-tanks-update.timer.d
  printf '%s\n' '[Timer]' 'OnUnitActiveSec=' 'OnUnitActiveSec=15s' 'RandomizedDelaySec=0' \
    'AccuracySec=1s' >"$timer_override"
else
  rm -f "$timer_override"
fi
systemctl daemon-reload
# Every machine follows its environment's tag by default; `pnpm run server:auto-update off`
# leaves a marker that keeps it off through later provisioning. The restart applies a
# changed interval.
if [[ ! -f /var/lib/sloppy-tanks/auto-update-off ]]; then
  systemctl enable sloppy-tanks-update.timer
  systemctl restart sloppy-tanks-update.timer
fi

# Caddy reloads its config in place, keeping players connected, unless its unit names
# another image than the running container's (a Caddy upgrade), which needs a restart.
# Podman reports the image as name@digest, without the tag.
if [[ $(podman inspect --format '{{.ImageName}}' caddy 2>/dev/null) == *"@${caddy_image##*@}" ]]; then
  systemctl reload-or-restart caddy
else
  systemctl restart caddy
fi
# Quadlet generates the server's service only once a deploy has pinned an image.
if [[ -f /etc/containers/systemd/sloppy-tanks.container.d/image.conf ]]; then
  if $server_unit_changed; then
    systemctl restart sloppy-tanks
  else
    systemctl start sloppy-tanks
  fi
fi

# Caddy needs 80 for the Let's Encrypt HTTP challenge and 443 for wss. The containers use
# host networking, so Podman publishes no ports around ufw.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable
