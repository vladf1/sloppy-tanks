#!/usr/bin/env bash
# A game server machine's first-boot setup, run by cloud-init from the provider's user
# data (deploy/server/cloud-init/), so a new machine needs no SSH from a developer's Mac.
# The user data writes /etc/sloppy-tanks/server.env first: SERVER_ENVIRONMENT (production
# or dev), DIRECTORY_URL and, for names beyond the machine's own nip.io name,
# EXTRA_HOSTNAMES. This fetches deploy/server/ from the repository at SLOPPY_REF (main),
# serves the nip.io name of the machine's IPv4 address, runs provision.sh and starts CI's
# :production image; the server then appears in the directory under its environment.
set -euo pipefail
SOURCE="https://raw.githubusercontent.com/vladf1/sloppy-tanks/${SLOPPY_REF:-main}/deploy/server"
FILES=(provision.sh Caddyfile caddy.container sloppy-tanks.container
  sloppy-tanks-update sloppy-tanks-update.service sloppy-tanks-update.timer)

work=$(mktemp -d)
for file in "${FILES[@]}"; do
  curl -fsSL --retry 5 -o "$work/$file" "$SOURCE/$file"
done

# shellcheck source=/dev/null
. /etc/sloppy-tanks/server.env
# The address the machine reaches the internet from, which nip.io names and the
# directory lists it by.
ip=$(ip -4 route get 1.1.1.1 | sed -n 's/.* src \([0-9.]*\).*/\1/p')
sites="${ip//./-}.nip.io${EXTRA_HOSTNAMES:+, $EXTRA_HOSTNAMES}"
sed -i "s/{\$SITES}/$sites/" "$work/Caddyfile"

bash "$work/provision.sh"
# A fresh machine has no pinned image; one that has keeps running it.
[[ -f /var/lib/sloppy-tanks/image ]] || sloppy-tanks-update pull
rm -rf "$work"
