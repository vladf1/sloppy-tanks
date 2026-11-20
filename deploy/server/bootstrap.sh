#!/usr/bin/env bash
# A game server machine's whole setup, run as root by cloud-init at its first boot from the
# provider's user data (deploy/server/cloud-init/), so a machine needs no SSH from a
# developer's Mac: a change here reaches a machine when it is created again. The user data
# writes /etc/sloppy-tanks/server.env first (SERVER_ENVIRONMENT, production, dev or
# app-store, and DIRECTORY_URL), which the servers' units and the updater read. This fetches the other
# files in deploy/server/ from the repository at SLOPPY_REF (main), installs Podman, the
# server's unit template, Caddy's Quadlet unit and the image updater, allows key-only SSH
# and only SSH, 80 and 443 through ufw, serves the nip.io name of the machine's IPv4
# address and starts a server for each content version its environment's deployment file
# lists (CI's for production, dev's publishes, the TestFlight builds' for app-store,
# scripts/deployment.mjs); each server then appears
# in the directory under its environment.
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
SOURCE="https://raw.githubusercontent.com/vladf1/sloppy-tanks/${SLOPPY_REF:-main}/deploy/server"
FILES=(caddy.container sloppy-tanks.container
  sloppy-tanks-update sloppy-tanks-update.service sloppy-tanks-update.timer)

work=$(mktemp -d)
for file in "${FILES[@]}"; do
  curl -fsSL --retry 5 -o "$work/$file" "$SOURCE/$file"
done

# The address the machine reaches the internet from, which nip.io names and the
# directory lists it by.
ip=$(ip -4 route get 1.1.1.1 | sed -n 's/.* src \([0-9.]*\).*/\1/p')

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
# The name Caddy serves and obtains a certificate for; the updater writes the Caddyfile
# around it, with a route for each version it runs.
echo "${ip//./-}.nip.io" >/etc/sloppy-tanks/site
install -m 755 "$work/sloppy-tanks-update" /usr/local/bin/sloppy-tanks-update
install -m 644 "$work/sloppy-tanks-update.service" "$work/sloppy-tanks-update.timer" \
  /etc/systemd/system/
# The server's unit is a template the updater fills in for each version.
install -m 644 "$work/sloppy-tanks.container" /etc/sloppy-tanks/
install -m 644 "$work/caddy.container" /etc/containers/systemd/
sloppy-tanks-update caddyfile
systemctl daemon-reload
systemctl enable --now sloppy-tanks-update.timer
# Caddy's unit pulls its pinned image at this first start; later starts find it here.
systemctl start caddy

# Caddy needs 80 for the Let's Encrypt HTTP challenge and 443 for wss. The containers use
# host networking, so Podman publishes no ports around ufw.
ufw allow OpenSSH
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# The first pull starts a server for each version the deployment file lists. A dev or App Store machine
# created before anything was published to its tag starts production's image, which
# auto-update replaces at the first publish. Should the registry not answer now, the rest
# of the setup stands and auto-update starts the servers once it can read the file.
[[ -f /var/lib/sloppy-tanks/wanted ]] || sloppy-tanks-update pull ||
  echo "Could not pull a server image; auto-update starts one once it can read its tag" >&2
rm -rf "$work"
