# Sourced by provision.sh and provision-dev.sh: the container runtime that runs the
# servers, the image updater with its auto-update timer (which starts disabled), and the
# functions that put each server's unit on that runtime.
#
# Podman runs the servers through Quadlet units (*.container); Docker, through units that
# wrap `docker run` (*.service), is the fallback. The provisioning script's first argument
# picks one; without it the host keeps the runtime its servers use, and a new host gets
# Podman. Switching carries each service's pinned images over, so it needs no registry.
runtime=${1:-}
if [[ -z $runtime ]]; then
  if compgen -G '/etc/containers/systemd/sloppy-tanks*.container' >/dev/null; then
    runtime=podman
  elif grep -qs 'docker run' /etc/systemd/system/sloppy-tanks.service /etc/systemd/system/sloppy-tanks-dev.service; then
    runtime=docker
  else
    runtime=podman
  fi
fi
case $runtime in
  podman) . ./install-podman.sh ;;
  docker) . ./install-docker.sh ;;
  *)
    echo "Unknown container runtime '$runtime' (podman or docker)" >&2
    exit 2
    ;;
esac
echo "Container runtime: $runtime"

# Shared by both services: updating it from a dev provision also changes how
# production updates.
install -m 755 sloppy-tanks-update /usr/local/bin/sloppy-tanks-update
install -d -m 755 /var/lib/sloppy-tanks
install -m 644 sloppy-tanks-update.service /etc/systemd/system/sloppy-tanks-update.service
install -m 644 sloppy-tanks-update.timer /etc/systemd/system/sloppy-tanks-update.timer

# Installs UNIT for the runtime, stopping and removing the other runtime's unit. The
# service stays down until start_service, after systemctl daemon-reload.
install_service() {
  local unit=$1 quadlet=/etc/containers/systemd/$1.container
  if [[ $runtime == podman ]]; then
    # A unit in /etc/systemd/system would shadow the one Quadlet generates. This also
    # retires the former binary unit; the deploy after provisioning starts the server.
    if [[ -f /etc/systemd/system/$unit.service ]]; then
      systemctl disable --now "$unit.service"
      rm -f "/etc/systemd/system/$unit.service"
    fi
    install -D -m 644 "$unit.container" "$quadlet"
  else
    if [[ -f $quadlet ]]; then
      systemctl stop "$unit"
      rm -rf "$quadlet" "$quadlet.d"
    fi
    install -m 644 "$unit.service" "/etc/systemd/system/$unit.service"
  fi
}

# Starts SERVICE (production or dev) on its pinned image, which the updater first
# copies from the other runtime if the service just switched. A service starts once a
# deploy has pinned its image; until then, a server started by the former binary unit
# under Docker keeps running and the first deploy replaces it.
start_service() {
  local service=$1 unit=$2
  # Quadlet's [Install] section enables its generated unit at every daemon-reload.
  if [[ $runtime == docker ]]; then systemctl enable "$unit"; fi
  if [[ -f /var/lib/sloppy-tanks/$service.image ]]; then sloppy-tanks-update "$service" adopt; fi
}

# Docker's daemons hold over 100 MB on the 949 MB host. Once no server uses them they
# stop; `--runtime docker` starts them again. docker.io stays installed for that until
# it is removed by hand.
stop_unused_docker() {
  if [[ $runtime == podman ]] && command -v docker >/dev/null &&
    ! grep -qs 'docker run' /etc/systemd/system/sloppy-tanks.service /etc/systemd/system/sloppy-tanks-dev.service; then
    systemctl disable --now docker.socket docker.service containerd.service
  fi
}
