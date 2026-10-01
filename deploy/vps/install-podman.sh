# Sourced by install-runtime.sh: Podman from Ubuntu's archive. It runs no daemon:
# systemd starts each server through its Quadlet unit, and a small conmon process
# watches each container. The recommended packages (Buildah, CRIU, docker-compose,
# GnuPG, rootless networking) are not needed to run a prebuilt image as root.

# python3 parses /stats for auto-update; Ubuntu server images already ship it.
apt-get install -y --no-install-recommends podman python3
