# Sourced by provision.sh and provision-dev.sh: Podman from Ubuntu's archive, the image
# updater with its auto-update timer (which starts disabled), and the functions that
# install and start each server's Quadlet unit.
#
# Podman runs no daemon: systemd starts each server through the unit Quadlet generates
# from its .container file, and a small conmon process watches each container. The
# recommended packages (Buildah, CRIU, docker-compose, GnuPG, rootless networking) are
# not needed to run a prebuilt image as root.

# python3 parses /stats for auto-update; Ubuntu server images already ship it.
apt-get install -y --no-install-recommends podman python3

# Shared by both services: updating it from a dev provision also changes how
# production updates.
install -m 755 sloppy-tanks-update /usr/local/bin/sloppy-tanks-update
install -d -m 755 /var/lib/sloppy-tanks
install -m 644 sloppy-tanks-update.service /etc/systemd/system/sloppy-tanks-update.service
install -m 644 sloppy-tanks-update.timer /etc/systemd/system/sloppy-tanks-update.timer

# Installs UNIT's Quadlet file. Takes effect at the next systemctl daemon-reload.
install_service() {
  install -D -m 644 "$1.container" "/etc/containers/systemd/$1.container"
}

# Restarts UNIT on its pinned image after daemon-reload. Quadlet generates no service
# until a deploy has pinned an image (sloppy-tanks-update writes the drop-in), and its
# [Install] section enables the generated one.
start_service() {
  if [[ -f /etc/containers/systemd/$1.container.d/image.conf ]]; then systemctl restart "$1"; fi
}
