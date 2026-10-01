# Sourced by provision.sh and provision-dev.sh: Docker Engine, its settings and the
# image updater with its auto-update timer, which starts disabled.
#
# daemon.json turns off Docker's bridge network and firewall rules: the servers use host
# networking, so Docker has no ports to publish around ufw. Its local log driver keeps
# container output bounded; the systemd journal also receives it through each unit.

# python3 parses /stats for auto-update; Ubuntu server images already ship it.
apt-get install -y docker.io python3
install -d -m 755 /etc/docker
if ! cmp -s daemon.json /etc/docker/daemon.json; then
  install -m 644 daemon.json /etc/docker/daemon.json
  # Restarting Docker stops running containers; their units start them again.
  systemctl restart docker
fi
systemctl enable --now docker
# Shared by both services: updating it from a dev provision also changes how
# production updates.
install -m 755 sloppy-tanks-update /usr/local/bin/sloppy-tanks-update
install -d -m 755 /var/lib/sloppy-tanks
install -m 644 sloppy-tanks-update.service /etc/systemd/system/sloppy-tanks-update.service
install -m 644 sloppy-tanks-update.timer /etc/systemd/system/sloppy-tanks-update.timer
